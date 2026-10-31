//! Convert to Word Document: Butter Paper's own PDF to Word (`.docx`)
//! converter, with no Word, LibreOffice or other converter installed.
//!
//! Each page is read as Edit PDF Content reads it: paragraphs with their
//! runs (font family, size, bold, italic, underline, colour), alignment and
//! line spacing; tables the structure tree declares, and grids the page
//! rules on all sides; and the images its content draws. They are written
//! as flowing Word content in reading order, top to bottom: paragraphs as
//! paragraphs (never text boxes or frames), tables as Word tables on their
//! column grid with merged cells kept, images inline. The space above each
//! comes from the gap above it on the page; each page is a section of its
//! own size and margins. Fonts are named, not embedded. Columns, lists,
//! headers and footers, and tables without rules are not recognised yet:
//! their text flows as plain paragraphs.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use lopdf::{Document, Object, ObjectId};

use crate::pdf_tables::{TableCell, line_core_box, page_tables};
use crate::pdf_text_blocks::{PageTextBlock, TextBlockAlignment, paragraph_blocks_in};
use crate::pdf_text_edit::{
    PageImage, PageTextLayer, PageTextLine, TextEditLimits, extract_page_text_and_images,
};

/// Points to twentieths of a point (Word's twips).
fn twips(points: f64) -> i64 {
    (points * 20.).round() as i64
}

/// Points to English Metric Units (drawing sizes).
fn emu(points: f64) -> i64 {
    (points * 12_700.).round() as i64
}

/// Where a PDF's Word document goes: beside it, with its name and a
/// `.docx` extension.
pub fn docx_path_for(pdf: &Path) -> PathBuf {
    pdf.with_file_name(format!("{}.docx", stem(pdf)))
}

/// Beside the PDF without replacing a file: [`docx_path_for`] while that is
/// free, then the first free `Name 2.docx`, `Name 3.docx`, ….
pub fn unused_docx_path_for(pdf: &Path) -> PathBuf {
    let stem = stem(pdf);
    std::iter::once(docx_path_for(pdf))
        .chain((2..).map(|n| pdf.with_file_name(format!("{stem} {n}.docx"))))
        .find(|path| !path.exists())
        .expect("an unused name exists")
}

fn stem(pdf: &Path) -> String {
    pdf.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".to_owned())
}

/// Converts the PDF at `pdf` into a Word document at `docx`, replacing a
/// file there. The document is written beside it first and moved into
/// place, so a failed conversion leaves any earlier one as it was.
pub fn convert(pdf: &Path, docx: &Path) -> Result<(), String> {
    let bytes =
        std::fs::read(pdf).map_err(|error| format!("The PDF could not be read: {error}"))?;
    // The reader is careful, but a PDF is outside data: a panic in it is a
    // failed conversion, not a crashed app.
    let converted = std::panic::catch_unwind(|| docx_from_pdf(&bytes))
        .map_err(|_| "The PDF could not be converted to a Word document.".to_owned())??;
    let name = docx
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document.docx".to_owned());
    let partial = docx.with_file_name(format!(".{name}.part"));
    std::fs::write(&partial, &converted)
        .map_err(|error| format!("The Word document could not be saved: {error}"))?;
    std::fs::rename(&partial, docx).map_err(|error| {
        let _ = std::fs::remove_file(&partial);
        format!(
            "The Word document could not be saved over {name}; close it in any app showing \
             it and try again ({error})."
        )
    })
}

/// The Word document for PDF `pdf`.
pub fn docx_from_pdf(pdf: &[u8]) -> Result<Vec<u8>, String> {
    let document =
        Document::load_mem(pdf).map_err(|error| format!("The PDF could not be read: {error}"))?;
    if document.is_encrypted() {
        return Err("The PDF needs a password to open, so it cannot be converted.".into());
    }
    let count = document.get_pages().len();
    let mut media = Vec::new();
    let pages = (0..count)
        .map(|index| read_page(&document, index as u32, &mut media))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(write_docx(&pages, &media))
}

// --- The page model -------------------------------------------------------

/// A page as Word content.
struct Page {
    /// Left, bottom, right, top.
    page_box: [f64; 4],
    items: Vec<Item>,
}

enum Item {
    Paragraph(Para),
    Table(Table),
    Image(Image),
}

impl Item {
    /// Left, bottom, right, top on the page.
    fn bounds(&self) -> [f64; 4] {
        match self {
            Item::Paragraph(para) => [para.left, para.bottom, para.right, para.top],
            Item::Table(table) => table.bounds,
            Item::Image(image) => image.rect,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RunStyle {
    font: String,
    /// In points.
    size: f64,
    bold: bool,
    italic: bool,
    underline: bool,
    colour: Option<[u8; 3]>,
}

struct Run {
    style: RunStyle,
    /// `\n` is a line break within the paragraph.
    text: String,
}

struct Para {
    runs: Vec<Run>,
    alignment: TextBlockAlignment,
    /// Baseline to baseline, in points; `None` for a single line.
    pitch: Option<f64>,
    /// Where its lines start (the leftmost) and its first line starts.
    left: f64,
    first_left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

struct Table {
    /// Column edges, left to right, and row edges, top to bottom.
    columns: Vec<f64>,
    rows: Vec<f64>,
    cells: Vec<(TableCell, Vec<Para>)>,
    /// Whether the page draws its grid.
    ruled: bool,
    bounds: [f64; 4],
}

struct Image {
    rect: [f64; 4],
    /// Into the document's media.
    media: usize,
}

struct Media {
    bytes: Vec<u8>,
    extension: &'static str,
}

fn read_page(document: &Document, index: u32, media: &mut Vec<Media>) -> Result<Page, String> {
    let (layer, images) = extract_page_text_and_images(document, index, &TextEditLimits::default())
        .map_err(|error| format!("Page {} could not be read: {error:?}", index + 1))?;
    let page_box = layer.page_box.unwrap_or([0., 0., 612., 792.]);
    // Declared tables and the grids the page rules (an untagged page's).
    let declared = page_tables(&layer);
    let blocks = paragraph_blocks_in(&layer, &declared);
    let tables = declared
        .iter()
        .map(|table| {
            let ruled = crate::pdf_tables::ruled_outline(&layer.rules, table.bounds());
            (table.clone(), ruled)
        })
        .collect::<Vec<_>>();
    let mut cell_paras: Vec<Vec<Vec<Para>>> = tables
        .iter()
        .map(|(table, _)| table.cells.iter().map(|_| Vec::new()).collect())
        .collect();
    let mut items = Vec::new();
    let mut place = |para: Para, block: Option<&PageTextBlock>, anchor: &PageTextLine| {
        let found = block
            .and_then(|block| block.cell)
            .and_then(|cell| {
                let table = tables
                    .iter()
                    .position(|(table, _)| table.key() == cell.table)?;
                Some((table, cell.cell))
            })
            .or_else(|| {
                let [left, bottom, right, top] = line_core_box(anchor)?;
                let (x, y) = ((left + right) / 2., (bottom + top) / 2.);
                tables
                    .iter()
                    .enumerate()
                    .find_map(|(index, (table, _))| Some((index, table.cell_at(x, y)?)))
            });
        match found {
            Some((table, cell)) => cell_paras[table][cell].push(para),
            None => items.push(Item::Paragraph(para)),
        }
    };
    for block in &blocks {
        let lines = block
            .lines
            .iter()
            .filter_map(|anchor| layer.line(anchor))
            .collect::<Vec<_>>();
        let Some(first) = lines.first().copied() else {
            continue;
        };
        let breaks =
            crate::pdf_text_reflow::Paragraph::new(std::sync::Arc::new(layer.clone()), block)
                .map(|paragraph| paragraph.hard_breaks().to_vec())
                .unwrap_or_else(|| vec![false; lines.len()]);
        if let Some(para) = paragraph(&layer, &lines, &breaks, block.alignment, block.pitch) {
            place(para, Some(block), first);
        }
    }
    // Text Edit PDF cannot change still converts, a line at a time.
    for line in layer
        .lines
        .iter()
        .filter(|line| line.read_only.is_some() && !line.text.trim().is_empty())
    {
        if let Some(para) = paragraph(&layer, &[line], &[false], TextBlockAlignment::Left, None) {
            place(para, None, line);
        }
    }
    for ((table, ruled), paras) in tables.into_iter().zip(cell_paras) {
        let cells = table
            .cells
            .iter()
            .cloned()
            .zip(paras.into_iter().map(|mut paras| {
                paras.sort_by(|a, b| b.top.total_cmp(&a.top));
                paras
            }))
            .collect();
        items.push(Item::Table(Table {
            bounds: table.bounds(),
            columns: table.columns,
            rows: table.rows,
            cells,
            ruled,
        }));
    }
    for image in images {
        if let Some(item) = image_item(document, image, media) {
            items.push(item);
        }
    }
    // Reading order: top to bottom, then left to right.
    items.sort_by(|a, b| {
        let (a, b) = (a.bounds(), b.bounds());
        b[3].total_cmp(&a[3]).then(a[0].total_cmp(&b[0]))
    });
    Ok(Page { page_box, items })
}

/// The paragraph of `lines`, joined as Edit PDF joins them: a wrap is a
/// space (unless the line ends in one), a hard break a line break.
fn paragraph(
    layer: &PageTextLayer,
    lines: &[&PageTextLine],
    breaks: &[bool],
    alignment: TextBlockAlignment,
    pitch: Option<f64>,
) -> Option<Para> {
    let mut runs: Vec<Run> = Vec::new();
    let mut push = |style: &RunStyle, text: &str| match runs.last_mut() {
        Some(run) if run.style == *style => run.text.push_str(text),
        _ => runs.push(Run {
            style: style.clone(),
            text: text.to_owned(),
        }),
    };
    let (mut left, mut right) = (f64::MAX, f64::MIN);
    let mut first_left = None;
    for (index, line) in lines.iter().enumerate() {
        let underline = underlined(layer, line);
        let visible = line
            .chars
            .iter()
            .enumerate()
            .filter(|(_, character)| !character.character.is_whitespace())
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let (Some(&start), Some(&end)) = (visible.first(), visible.last()) else {
            continue;
        };
        let [line_left, _, line_right, _] = crate::pdf_tables::line_box(line)?;
        left = left.min(line_left);
        right = right.max(line_right);
        first_left.get_or_insert(line_left);
        let mut last_style = None;
        for character in &line.chars[start..=end] {
            let style = run_style(layer, line, character.style, underline);
            push(&style, &character.character.to_string());
            last_style = Some(style);
        }
        if index + 1 < lines.len()
            && let Some(style) = last_style
        {
            let hard = breaks.get(index).copied().unwrap_or(false);
            push(&style, if hard { "\n" } else { " " });
        }
    }
    let first = lines.first()?;
    let last = lines.last()?;
    let edge = |line: &PageTextLine, top: bool| {
        line.quad(0., line.end_x).iter().map(|point| point.1).fold(
            if top { f64::MIN } else { f64::MAX },
            |a, b| {
                if top { a.max(b) } else { a.min(b) }
            },
        )
    };
    (!runs.is_empty()).then(|| Para {
        runs,
        alignment,
        pitch: pitch.filter(|_| lines.len() > 1),
        left,
        first_left: first_left.unwrap_or(left),
        right,
        top: edge(first, true),
        bottom: edge(last, false),
    })
}

/// Whether the page underlines `line`: a rule just below its baseline
/// along most of its letters.
fn underlined(layer: &PageTextLayer, line: &PageTextLine) -> bool {
    let Some([left, _, right, _]) = crate::pdf_tables::line_box(line) else {
        return false;
    };
    let m = line.matrix;
    if m[1].abs() > 1e-6 || m[0] <= 0. {
        return false;
    }
    let size = line.chars.first().map_or(12., |character| {
        line.styles[character.style].font_size.abs()
    }) * m[3].hypot(m[2]);
    let (_, baseline) = line.origin();
    layer.rules.iter().any(|rule| {
        rule.horizontal
            && rule.at < baseline
            && rule.at > baseline - 0.3 * size
            && rule.from <= left + 0.1 * (right - left)
            && rule.to >= right - 0.1 * (right - left)
            && rule.to - rule.from <= (right - left) * 1.2 + size
    })
}

fn run_style(
    layer: &PageTextLayer,
    line: &PageTextLine,
    style: usize,
    underline: bool,
) -> RunStyle {
    let style = &line.styles[style];
    let metrics = layer.font(&style.font);
    let (postscript, fallback) = match &style.face {
        Some(face) => (face.postscript_name.clone(), Some(face.fallback)),
        None => (
            metrics.and_then(|font| font.postscript_name.clone()),
            metrics.map(|font| font.fallback),
        ),
    };
    let (family, named_bold, named_italic) = font_family(
        postscript.as_deref().unwrap_or(""),
        fallback.map_or(crate::pdf_text_edit::FallbackFamily::Sans, |face| {
            face.family
        }),
    );
    let m = line.matrix;
    let colour = style
        .fill
        .or(line.fill)
        .map(|[r, g, b]| [r, g, b].map(|component| (component.clamp(0., 1.) * 255.).round() as u8));
    RunStyle {
        font: family,
        size: (style.font_size.abs() * m[3].hypot(m[2]) * 2.).round() / 2.,
        bold: named_bold || fallback.is_some_and(|face| face.bold),
        italic: named_italic || fallback.is_some_and(|face| face.italic),
        underline,
        colour: colour.filter(|colour| *colour != [0, 0, 0]),
    }
}

/// A font's family name, and whether its name says bold and italic, from its
/// PostScript name: `ArialMT` is Arial, `TimesNewRomanPS-BoldMT` Times New
/// Roman bold, `LiberationSans-Italic` Liberation Sans italic. A name that
/// says nothing (`F1`, a subset's `CIDFont+F2`) is the matching common
/// family.
fn font_family(
    postscript: &str,
    fallback: crate::pdf_text_edit::FallbackFamily,
) -> (String, bool, bool) {
    let name = postscript.rsplit('+').next().unwrap_or(postscript);
    let (base, style) = match name.find(['-', ',']) {
        Some(at) => (&name[..at], &name[at + 1..]),
        None => (name, ""),
    };
    let lower = name.to_ascii_lowercase();
    let style_lower = style.to_ascii_lowercase();
    let bold = ["bold", "black", "heavy", "semibold", "demi"]
        .iter()
        .any(|word| style_lower.contains(word));
    let italic = style_lower.contains("italic") || style_lower.contains("oblique");
    let mut base = base.to_owned();
    for suffix in ["PSMT", "PS", "MT"] {
        if base.len() > suffix.len() && base.ends_with(suffix) {
            base.truncate(base.len() - suffix.len());
            break;
        }
    }
    // Words of a run-together name: `TimesNewRoman`, `LiberationSans`.
    let mut family = String::new();
    let chars = base.chars().collect::<Vec<_>>();
    for (index, character) in chars.iter().enumerate() {
        let previous = index.checked_sub(1).map(|index| chars[index]);
        let next = chars.get(index + 1);
        if index > 0
            && character.is_ascii_uppercase()
            && (previous.is_some_and(|previous| previous.is_ascii_lowercase())
                || previous.is_some_and(|previous| previous.is_ascii_uppercase())
                    && next.is_some_and(|next| next.is_ascii_lowercase()))
        {
            family.push(' ');
        }
        family.push(*character);
    }
    let meaningless = family.len() < 3
        || lower.starts_with("cidfont")
        || (family.starts_with(['F', 'T', 'R'])
            && family[1..]
                .chars()
                .all(|character| character.is_ascii_digit()));
    if meaningless {
        family = match fallback {
            crate::pdf_text_edit::FallbackFamily::Sans => "Arial",
            crate::pdf_text_edit::FallbackFamily::Serif => "Times New Roman",
            crate::pdf_text_edit::FallbackFamily::Mono => "Courier New",
        }
        .to_owned();
    }
    (family, bold, italic)
}

// --- Images --------------------------------------------------------------

/// The image `image` draws, as media the document embeds: a JPEG as it is,
/// anything else PNG. Images the reader cannot decode are left out.
fn image_item(document: &Document, image: PageImage, media: &mut Vec<Media>) -> Option<Item> {
    let [left, bottom, right, top] = image.rect;
    if right - left < 4. || top - bottom < 4. {
        return None;
    }
    let (bytes, extension) = image_bytes(document, image.object)?;
    media.push(Media { bytes, extension });
    Some(Item::Image(Image {
        rect: image.rect,
        media: media.len() - 1,
    }))
}

fn resolved<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    document.dereference(object).ok().map(|(_, object)| object)
}

/// A colour space's component count and, for an indexed one, its palette
/// as RGB.
fn colour_space(document: &Document, space: &Object) -> Option<(usize, Option<Vec<[u8; 3]>>)> {
    let space = resolved(document, space)?;
    match space {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"CalGray" | b"G" => Some((1, None)),
            b"DeviceRGB" | b"CalRGB" | b"RGB" => Some((3, None)),
            b"DeviceCMYK" | b"CMYK" => Some((4, None)),
            _ => None,
        },
        Object::Array(items) => {
            let kind = resolved(document, items.first()?)?.as_name().ok()?;
            match kind {
                b"ICCBased" => {
                    let stream = resolved(document, items.get(1)?)?.as_stream().ok()?;
                    let count = stream.dict.get(b"N").ok()?.as_i64().ok()?;
                    matches!(count, 1 | 3 | 4).then_some((count as usize, None))
                }
                b"CalGray" => Some((1, None)),
                b"CalRGB" => Some((3, None)),
                b"Indexed" | b"I" => {
                    let (base, _) = colour_space(document, items.get(1)?)?;
                    let high = resolved(document, items.get(2)?)?.as_i64().ok()? as usize;
                    let table = match resolved(document, items.get(3)?)? {
                        Object::String(bytes, _) => bytes.clone(),
                        Object::Stream(stream) => stream.decompressed_content().ok()?,
                        _ => return None,
                    };
                    let palette = (0..=high.min(255))
                        .map(|index| {
                            let entry = table.get(index * base..(index + 1) * base)?;
                            Some(to_rgb(entry, base))
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some((1, Some(palette)))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn to_rgb(components: &[u8], count: usize) -> [u8; 3] {
    match count {
        1 => [components[0]; 3],
        3 => [components[0], components[1], components[2]],
        _ => {
            let k = components[3] as f64 / 255.;
            [0, 1, 2].map(|index| {
                (255. * (1. - components[index] as f64 / 255.) * (1. - k)).round() as u8
            })
        }
    }
}

fn filters(dict: &lopdf::Dictionary, document: &Document) -> Vec<Vec<u8>> {
    match dict
        .get(b"Filter")
        .ok()
        .and_then(|filter| resolved(document, filter))
    {
        Some(Object::Name(name)) => vec![name.clone()],
        Some(Object::Array(items)) => items
            .iter()
            .filter_map(|item| resolved(document, item)?.as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    }
}

fn image_bytes(document: &Document, object: ObjectId) -> Option<(Vec<u8>, &'static str)> {
    let stream = document.get_object(object).ok()?.as_stream().ok()?;
    let dict = &stream.dict;
    let number = |key: &[u8]| {
        dict.get(key)
            .ok()
            .and_then(|value| resolved(document, value))
            .and_then(|value| value.as_i64().ok())
    };
    let (width, height) = (number(b"Width")? as u32, number(b"Height")? as u32);
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 50_000_000 {
        return None;
    }
    if dict
        .get(b"ImageMask")
        .ok()
        .and_then(|mask| mask.as_bool().ok())
        == Some(true)
    {
        return None;
    }
    let (components, palette) = colour_space(document, dict.get(b"ColorSpace").ok()?)?;
    let has_mask = dict.get(b"SMask").is_ok();
    let filters = filters(dict, document);
    // A plain JPEG embeds as it is.
    if filters == [b"DCTDecode".to_vec()] && matches!(components, 1 | 3) && !has_mask {
        return Some((stream.content.clone(), "jpeg"));
    }
    let pixels = if filters.last().is_some_and(|filter| filter == b"DCTDecode") {
        let jpeg = if filters.len() == 1 {
            stream.content.clone()
        } else {
            let mut copy = stream.clone();
            copy.dict.set(
                "Filter",
                Object::Array(
                    filters[..filters.len() - 1]
                        .iter()
                        .map(|name| Object::Name(name.clone()))
                        .collect(),
                ),
            );
            copy.decompressed_content().ok()?
        };
        image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
            .ok()?
            .to_rgb8()
    } else if filters.iter().all(|filter| {
        matches!(
            filter.as_slice(),
            b"FlateDecode" | b"Fl" | b"LZWDecode" | b"ASCIIHexDecode" | b"ASCII85Decode"
        )
    }) {
        let data = if filters.is_empty() {
            stream.content.clone()
        } else {
            stream.decompressed_content().ok()?
        };
        let bits = number(b"BitsPerComponent").unwrap_or(8);
        rgb_pixels(
            &data,
            width,
            height,
            components,
            bits as u32,
            palette.as_deref(),
        )?
    } else {
        return None;
    };
    let alpha = has_mask
        .then(|| {
            let mask = resolved(document, dict.get(b"SMask").ok()?)?
                .as_stream()
                .ok()?;
            let width = mask.dict.get(b"Width").ok()?.as_i64().ok()? as u32;
            let height = mask.dict.get(b"Height").ok()?.as_i64().ok()? as u32;
            let data = mask.decompressed_content().ok()?;
            let gray = image::GrayImage::from_raw(
                width,
                height,
                data.get(..(width * height) as usize)?.to_vec(),
            )?;
            Some(image::imageops::resize(
                &gray,
                pixels.width(),
                pixels.height(),
                image::imageops::FilterType::Triangle,
            ))
        })
        .flatten();
    let mut png = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png);
    match alpha {
        Some(alpha) => {
            let mut rgba = image::RgbaImage::new(pixels.width(), pixels.height());
            for (x, y, pixel) in rgba.enumerate_pixels_mut() {
                let [r, g, b] = pixels.get_pixel(x, y).0;
                *pixel = image::Rgba([r, g, b, alpha.get_pixel(x, y).0[0]]);
            }
            image::ImageEncoder::write_image(
                encoder,
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .ok()?;
        }
        None => image::ImageEncoder::write_image(
            encoder,
            pixels.as_raw(),
            pixels.width(),
            pixels.height(),
            image::ExtendedColorType::Rgb8,
        )
        .ok()?,
    }
    Some((png, "png"))
}

/// Decoded samples as RGB: 8 bits per component, or 1, 2 or 4 for a gray
/// or indexed image.
fn rgb_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    components: usize,
    bits: u32,
    palette: Option<&[[u8; 3]]>,
) -> Option<image::RgbImage> {
    let row_bytes = (width as usize * components * bits as usize).div_ceil(8);
    if data.len() < row_bytes * height as usize || !matches!(bits, 1 | 2 | 4 | 8) {
        return None;
    }
    if bits != 8 && components != 1 {
        return None;
    }
    let mut rgb = image::RgbImage::new(width, height);
    for y in 0..height as usize {
        let row = &data[y * row_bytes..(y + 1) * row_bytes];
        for x in 0..width as usize {
            let pixel = if bits == 8 {
                let sample = &row[x * components..(x + 1) * components];
                match palette {
                    Some(palette) => *palette.get(sample[0] as usize).unwrap_or(&[0; 3]),
                    None => to_rgb(sample, components),
                }
            } else {
                let per_byte = 8 / bits as usize;
                let byte = row[x / per_byte];
                let shift = 8 - bits as usize * (x % per_byte + 1);
                let value = (byte >> shift) & ((1u16 << bits) - 1) as u8;
                match palette {
                    Some(palette) => *palette.get(value as usize).unwrap_or(&[0; 3]),
                    None => [(value as u32 * 255 / ((1 << bits) - 1)) as u8; 3],
                }
            };
            rgb.put_pixel(x as u32, y as u32, image::Rgb(pixel));
        }
    }
    Some(rgb)
}

// --- WordprocessingML ----------------------------------------------------

/// Text for XML: escaped, without characters XML cannot hold.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\t' | '\n' | '\r' => escaped.push(character),
            character if (character as u32) < 0x20 => {}
            '\u{fffe}' | '\u{ffff}' => {}
            character => escaped.push(character),
        }
    }
    escaped
}

fn run_xml(run: &Run, out: &mut String) {
    let style = &run.style;
    let mut properties = format!(
        "<w:rFonts w:ascii=\"{0}\" w:hAnsi=\"{0}\" w:cs=\"{0}\" w:eastAsia=\"{0}\"/>",
        escape(&style.font)
    );
    if style.bold {
        properties.push_str("<w:b/><w:bCs/>");
    }
    if style.italic {
        properties.push_str("<w:i/><w:iCs/>");
    }
    if style.underline {
        properties.push_str("<w:u w:val=\"single\"/>");
    }
    if let Some([r, g, b]) = style.colour {
        properties.push_str(&format!("<w:color w:val=\"{r:02X}{g:02X}{b:02X}\"/>"));
    }
    let half_points = (style.size * 2.).round().max(2.) as i64;
    properties.push_str(&format!(
        "<w:sz w:val=\"{half_points}\"/><w:szCs w:val=\"{half_points}\"/>"
    ));
    out.push_str(&format!("<w:r><w:rPr>{properties}</w:rPr>"));
    for (index, piece) in run.text.split('\n').enumerate() {
        if index > 0 {
            out.push_str("<w:br/>");
        }
        for (index, part) in piece.split('\t').enumerate() {
            if index > 0 {
                out.push_str("<w:tab/>");
            }
            if !part.is_empty() {
                out.push_str(&format!(
                    "<w:t xml:space=\"preserve\">{}</w:t>",
                    escape(part)
                ));
            }
        }
    }
    out.push_str("</w:r>");
}

/// A paragraph's properties: alignment, spacing and indents relative to
/// the text area from `area_left` to `area_right`; `section` ends a page's
/// section with it.
fn para_xml(
    para: &Para,
    before: f64,
    area_left: f64,
    area_right: f64,
    section: Option<&str>,
    out: &mut String,
) {
    let mut properties = String::new();
    let line = match para.pitch {
        Some(pitch) => format!(" w:line=\"{}\" w:lineRule=\"atLeast\"", twips(pitch)),
        None => String::new(),
    };
    properties.push_str(&format!(
        "<w:spacing w:before=\"{}\" w:after=\"0\"{line}/>",
        twips(before.max(0.))
    ));
    // A single line's alignment shows only against the text area: one
    // ending at its right edge, or centred in it, keeps that place
    // whatever the font's widths in the word processor.
    let mut alignment = para.alignment;
    if para.pitch.is_none() && alignment == TextBlockAlignment::Left && para.left - area_left > 36.
    {
        if (para.right - area_right).abs() <= 2. {
            alignment = TextBlockAlignment::Right;
        } else if ((para.left + para.right) - (area_left + area_right)).abs() / 2. <= 2. {
            alignment = TextBlockAlignment::Center;
        }
    }
    // Lines start where they start on the page; wrapped text runs on to
    // the text area's right edge, as the font's widths there decide.
    if matches!(
        alignment,
        TextBlockAlignment::Left | TextBlockAlignment::Justified
    ) {
        let left = para.left - area_left;
        let first = para.first_left - para.left;
        if left.abs() >= 1. || first.abs() >= 1. {
            let first = if first >= 0. {
                format!(" w:firstLine=\"{}\"", twips(first))
            } else {
                format!(" w:hanging=\"{}\"", twips(-first))
            };
            properties.push_str(&format!("<w:ind w:left=\"{}\"{first}/>", twips(left)));
        }
    }
    let justification = match alignment {
        TextBlockAlignment::Left => "left",
        TextBlockAlignment::Right => "right",
        TextBlockAlignment::Center => "center",
        TextBlockAlignment::Justified => "both",
    };
    properties.push_str(&format!("<w:jc w:val=\"{justification}\"/>"));
    if let Some(section) = section {
        properties.push_str(section);
    }
    out.push_str(&format!("<w:p><w:pPr>{properties}</w:pPr>"));
    for run in &para.runs {
        run_xml(run, out);
    }
    out.push_str("</w:p>");
}

fn image_xml(
    image: &Image,
    before: f64,
    area_left: f64,
    id: usize,
    relationship: &str,
    section: Option<&str>,
    out: &mut String,
) {
    let [left, bottom, right, top] = image.rect;
    let (cx, cy) = (emu(right - left), emu(top - bottom));
    out.push_str(&format!(
        "<w:p><w:pPr><w:spacing w:before=\"{}\" w:after=\"0\"/><w:ind w:left=\"{}\"/>{}</w:pPr>\
         <w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
         <wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{id}\" name=\"Picture {id}\"/>\
         <wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"Picture {id}\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"{relationship}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
         </a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
        twips(before.max(0.)),
        twips((left - area_left).max(0.)),
        section.unwrap_or(""),
    ));
}

fn table_xml(table: &Table, area_left: f64, out: &mut String) {
    let widths = table
        .columns
        .windows(2)
        .map(|pair| twips(pair[1] - pair[0]))
        .collect::<Vec<_>>();
    // The cells' text inset, as the page sets it: the least gap from a
    // cell's left edge to its text.
    let inset = table
        .cells
        .iter()
        .flat_map(|(cell, paras)| {
            let left = table.columns[cell.column];
            paras.iter().map(move |para| para.left - left)
        })
        .filter(|gap| *gap >= 0.)
        .fold(f64::MAX, f64::min);
    let inset = if inset == f64::MAX {
        5.4
    } else {
        inset.min(20.)
    };
    let border = if table.ruled { "single" } else { "nil" };
    let borders = ["top", "left", "bottom", "right", "insideH", "insideV"]
        .iter()
        .map(|side| {
            format!("<w:{side} w:val=\"{border}\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>")
        })
        .collect::<String>();
    out.push_str(&format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"{}\" w:type=\"dxa\"/><w:tblInd w:w=\"{}\" w:type=\"dxa\"/>\
         <w:tblBorders>{borders}</w:tblBorders><w:tblLayout w:type=\"fixed\"/>\
         <w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"{2}\" w:type=\"dxa\"/>\
         <w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"{2}\" w:type=\"dxa\"/></w:tblCellMar>\
         <w:tblLook w:val=\"0000\"/></w:tblPr><w:tblGrid>",
        widths.iter().sum::<i64>(),
        twips(table.columns[0] - area_left),
        twips(inset),
    ));
    for width in &widths {
        out.push_str(&format!("<w:gridCol w:w=\"{width}\"/>"));
    }
    out.push_str("</w:tblGrid>");
    let row_count = table.rows.len() - 1;
    let column_count = table.columns.len() - 1;
    for row in 0..row_count {
        out.push_str(&format!(
            "<w:tr><w:trPr><w:trHeight w:val=\"{}\" w:hRule=\"atLeast\"/></w:trPr>",
            twips(table.rows[row] - table.rows[row + 1])
        ));
        let mut column = 0;
        while column < column_count {
            let found = table.cells.iter().find(|(cell, _)| {
                (cell.row..cell.row + cell.row_span).contains(&row)
                    && (cell.column..cell.column + cell.column_span).contains(&column)
            });
            let (span, merge, paras, cell_left, cell_right) = match found {
                Some((cell, paras)) => (
                    cell.column_span,
                    match (cell.row_span > 1, cell.row == row) {
                        (false, _) => "",
                        (true, true) => "<w:vMerge w:val=\"restart\"/>",
                        (true, false) => "<w:vMerge/>",
                    },
                    if cell.row == row {
                        paras.as_slice()
                    } else {
                        &[]
                    },
                    table.columns[cell.column],
                    table.columns[cell.column + cell.column_span],
                ),
                None => (
                    1,
                    "",
                    &[][..],
                    table.columns[column],
                    table.columns[column + 1],
                ),
            };
            let width: i64 = widths[column..column + span].iter().sum();
            let grid_span = if span > 1 {
                format!("<w:gridSpan w:val=\"{span}\"/>")
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"{width}\" w:type=\"dxa\"/>{grid_span}{merge}</w:tcPr>"
            ));
            if paras.is_empty() {
                out.push_str("<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\"/></w:pPr></w:p>");
            }
            let mut previous = table.rows[row] - 1.;
            for para in paras {
                let before = if std::ptr::eq(para, &paras[0]) {
                    table.rows[row] - para.top - 1.
                } else {
                    previous - para.top
                };
                para_xml(
                    para,
                    before,
                    cell_left + inset,
                    cell_right - inset,
                    None,
                    out,
                );
                previous = para.bottom;
            }
            out.push_str("</w:tc>");
            column += span;
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
}

/// A page's section properties: its size and margins.
fn section_xml(page: &Page, margins: [f64; 4]) -> String {
    let [left, bottom, right, top] = page.page_box;
    let (width, height) = (right - left, top - bottom);
    let orient = if width > height {
        " w:orient=\"landscape\""
    } else {
        ""
    };
    format!(
        "<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"{orient}/>\
         <w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/></w:sectPr>",
        twips(width),
        twips(height),
        twips(margins[3]),
        twips(margins[2]),
        twips(margins[1]),
        twips(margins[0]),
    )
}

/// The page's margins (left, bottom, right, top): its content's extent,
/// within reason.
fn margins(page: &Page) -> [f64; 4] {
    let [page_left, page_bottom, page_right, page_top] = page.page_box;
    let bounds = page.items.iter().map(Item::bounds);
    let (mut left, mut bottom, mut right, mut top) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for [l, b, r, t] in bounds {
        left = left.min(l);
        bottom = bottom.min(b);
        right = right.max(r);
        top = top.max(t);
    }
    if left == f64::MAX {
        return [72.; 4];
    }
    [
        (left - page_left).clamp(9., 180.),
        // Room for Word's line heights to run a little longer than the
        // PDF's.
        (bottom - page_bottom - 12.).clamp(9., 72.),
        (page_right - right).clamp(9., 180.),
        (page_top - top).clamp(9., 180.),
    ]
}

fn document_xml(pages: &[Page], media_relationships: &[String]) -> String {
    let mut body = String::new();
    let mut picture = 0;
    for (index, page) in pages.iter().enumerate() {
        let margins = margins(page);
        let area_left = page.page_box[0] + margins[0];
        let area_right = page.page_box[2] - margins[2];
        let section = section_xml(page, margins);
        let last_page = index + 1 == pages.len();
        // The last page's section is the body's; the others end with a
        // paragraph holding theirs.
        let ends_section =
            |item: usize| (!last_page && item + 1 == page.items.len()).then_some(section.as_str());
        let mut previous = page.page_box[3] - margins[3];
        for (item_index, item) in page.items.iter().enumerate() {
            let before = previous - item.bounds()[3];
            match item {
                Item::Paragraph(para) => para_xml(
                    para,
                    before,
                    area_left,
                    area_right,
                    ends_section(item_index),
                    &mut body,
                ),
                Item::Image(image) => {
                    picture += 1;
                    image_xml(
                        image,
                        before,
                        area_left,
                        picture,
                        &media_relationships[image.media],
                        ends_section(item_index),
                        &mut body,
                    )
                }
                Item::Table(table) => {
                    if before > 1. {
                        // The gap above a table, as an empty paragraph's
                        // spacing.
                        body.push_str(&format!(
                            "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"{}\" w:lineRule=\"exact\"/></w:pPr></w:p>",
                            twips(before.max(1.))
                        ));
                    }
                    table_xml(table, area_left, &mut body);
                    if let Some(section) = ends_section(item_index) {
                        body.push_str(&format!(
                            "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>{section}</w:pPr></w:p>"
                        ));
                    }
                }
            }
            previous = item.bounds()[1];
        }
        if page.items.is_empty() && !last_page {
            body.push_str(&format!("<w:p><w:pPr>{section}</w:pPr></w:p>"));
        }
        if last_page {
            if matches!(page.items.last(), Some(Item::Table(_))) {
                // A table cannot end the document.
                body.push_str("<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/></w:pPr></w:p>");
            }
            body.push_str(&section);
        }
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <w:body>{body}</w:body></w:document>"
    )
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Default Extension=\"png\" ContentType=\"image/png\"/>\
<Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
<Override PartName=\"/word/settings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml\"/>\
</Types>";

const PACKAGE_RELATIONSHIPS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";

/// Paragraphs carry their own spacing; nothing is added between them.
const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Arial\" w:hAnsi=\"Arial\" w:cs=\"Arial\" w:eastAsia=\"Arial\"/>\
<w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/><w:lang w:val=\"en-US\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/>\
<w:tblPr><w:tblInd w:w=\"0\" w:type=\"dxa\"/><w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/>\
<w:left w:w=\"108\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/>\
</w:tblCellMar></w:tblPr></w:style>\
</w:styles>";

/// Word opens it in its current layout mode, not compatibility mode.
const SETTINGS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/></w:compat>\
</w:settings>";

fn write_docx(pages: &[Page], media: &[Media]) -> Vec<u8> {
    let relationships = (0..media.len())
        .map(|index| format!("rId{}", index + 3))
        .collect::<Vec<_>>();
    let mut document_relationships = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
         <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" Target=\"settings.xml\"/>",
    );
    for (index, item) in media.iter().enumerate() {
        document_relationships.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{}.{}\"/>",
            relationships[index],
            index + 1,
            item.extension
        ));
    }
    document_relationships.push_str("</Relationships>");
    let mut entries = vec![
        (
            "[Content_Types].xml".to_owned(),
            CONTENT_TYPES.as_bytes().to_vec(),
            true,
        ),
        (
            "_rels/.rels".to_owned(),
            PACKAGE_RELATIONSHIPS.as_bytes().to_vec(),
            true,
        ),
        (
            "word/document.xml".to_owned(),
            document_xml(pages, &relationships).into_bytes(),
            true,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            document_relationships.into_bytes(),
            true,
        ),
        (
            "word/styles.xml".to_owned(),
            STYLES.as_bytes().to_vec(),
            true,
        ),
        (
            "word/settings.xml".to_owned(),
            SETTINGS.as_bytes().to_vec(),
            true,
        ),
    ];
    for (index, item) in media.iter().enumerate() {
        // Images are compressed already.
        entries.push((
            format!("word/media/image{}.{}", index + 1, item.extension),
            item.bytes.clone(),
            false,
        ));
    }
    zip(&entries)
}

// --- Zip -----------------------------------------------------------------

/// A zip archive of `entries` (name, contents, whether to deflate), as the
/// Open Packaging Conventions read one: no encryption, no Zip64.
fn zip(entries: &[(String, Vec<u8>, bool)]) -> Vec<u8> {
    // 1980-01-01, the earliest date zip can hold: the same bytes each time.
    const DATE: u16 = 0x0021;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, contents, deflate) in entries {
        let crc = crc32fast::hash(contents);
        let (method, data) = if *deflate {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(contents).expect("writing to memory");
            (8u16, encoder.finish().expect("writing to memory"))
        } else {
            (0u16, contents.clone())
        };
        let offset = out.len() as u32;
        let header = |signature: u32, central: bool| {
            let mut header = Vec::new();
            header.extend_from_slice(&signature.to_le_bytes());
            if central {
                header.extend_from_slice(&20u16.to_le_bytes());
            }
            header.extend_from_slice(&20u16.to_le_bytes());
            // UTF-8 names.
            header.extend_from_slice(&0x0800u16.to_le_bytes());
            header.extend_from_slice(&method.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&DATE.to_le_bytes());
            header.extend_from_slice(&crc.to_le_bytes());
            header.extend_from_slice(&(data.len() as u32).to_le_bytes());
            header.extend_from_slice(&(contents.len() as u32).to_le_bytes());
            header.extend_from_slice(&(name.len() as u16).to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            if central {
                // Comment, disk, internal and external attributes, offset.
                header.extend_from_slice(&0u16.to_le_bytes());
                header.extend_from_slice(&0u16.to_le_bytes());
                header.extend_from_slice(&0u16.to_le_bytes());
                header.extend_from_slice(&0u32.to_le_bytes());
                header.extend_from_slice(&offset.to_le_bytes());
            }
            header.extend_from_slice(name.as_bytes());
            header
        };
        out.extend(header(0x0403_4b50, false));
        out.extend_from_slice(&data);
        central.extend(header(0x0201_4b50, true));
    }
    let directory = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&directory.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_document_goes_beside_the_pdf() {
        let directory = std::env::temp_dir().join(format!(
            "bp-docx-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let pdf = directory.join("Initial report 15.10.26.pdf");
        assert_eq!(
            unused_docx_path_for(&pdf),
            directory.join("Initial report 15.10.26.docx")
        );
        std::fs::write(directory.join("Initial report 15.10.26.docx"), b"").unwrap();
        assert_eq!(
            docx_path_for(&pdf),
            directory.join("Initial report 15.10.26.docx")
        );
        assert_eq!(
            unused_docx_path_for(&pdf),
            directory.join("Initial report 15.10.26 2.docx")
        );
        std::fs::write(directory.join("Initial report 15.10.26 2.docx"), b"").unwrap();
        assert_eq!(
            unused_docx_path_for(&pdf),
            directory.join("Initial report 15.10.26 3.docx")
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn font_families_come_from_postscript_names() {
        use crate::pdf_text_edit::FallbackFamily::{Mono, Sans, Serif};
        assert_eq!(font_family("ArialMT", Sans), ("Arial".into(), false, false));
        assert_eq!(
            font_family("ABCDEF+TimesNewRomanPS-BoldItalicMT", Serif),
            ("Times New Roman".into(), true, true)
        );
        assert_eq!(
            font_family("LiberationSans-Bold", Sans),
            ("Liberation Sans".into(), true, false)
        );
        assert_eq!(
            font_family("Calibri,Italic", Sans),
            ("Calibri".into(), false, true)
        );
        assert_eq!(
            font_family("F1", Mono),
            ("Courier New".into(), false, false)
        );
        assert_eq!(font_family("", Serif).0, "Times New Roman");
    }

    #[test]
    fn text_is_escaped_for_xml() {
        assert_eq!(escape("a < b & \"c\"\u{1}"), "a &lt; b &amp; &quot;c&quot;");
    }
}
