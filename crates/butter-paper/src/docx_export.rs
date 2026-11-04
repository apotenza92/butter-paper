//! Convert to Word Document: Butter Paper's own PDF to Word (`.docx`)
//! converter, with no Word, LibreOffice or other converter installed.
//!
//! A page that reads as a document is read as Edit PDF Content reads it:
//! paragraphs with their runs (font family, size, bold, italic, underline,
//! colour), alignment and line spacing; tables the structure tree declares,
//! and grids the page rules on all sides; and the images its content draws.
//! They are written as flowing Word content in reading order, top to
//! bottom: paragraphs as paragraphs, tables as Word tables on their column
//! grid with merged cells kept, images inline. The space above each comes
//! from the gap above it on the page; each page is a section of its own
//! size and margins. Columns, lists, headers and footers, and tables
//! without rules are not recognised yet: their text flows as plain
//! paragraphs.
//!
//! A page flowing text cannot hold (a sheet larger than A3, a drawing or
//! map, text at an angle or over an image) keeps its layout instead
//! ([`crate::docx_fixed`]): its drawings and images are pictures behind its
//! text, paths as SVG, and each line of text is a text box at its place.
//! Either way the document has the PDF's pages, one section each. Fonts are
//! named, not embedded.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use lopdf::{Document, Object, ObjectId};

use crate::pdf_tables::{TableCell, line_core_box, page_tables};
use crate::pdf_text_blocks::{PageTextBlock, TextBlockAlignment, paragraph_blocks_in};
use crate::pdf_text_edit::{
    FontCache, PageImage, PageTextLayer, PageTextLine, TextEditLimits,
    extract_page_text_and_images_at,
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

/// `Name.docx` for the PDF at `pdf`, in `folder`.
pub fn docx_path_in(folder: &Path, pdf: &Path) -> PathBuf {
    folder.join(format!("{}.docx", stem(pdf)))
}

/// In `folder` without replacing a file: [`docx_path_in`] while that is
/// free, then the first free `Name 2.docx`, `Name 3.docx`, ….
pub fn unused_docx_path_in(folder: &Path, pdf: &Path) -> PathBuf {
    crate::save_places::unused_path_in(folder, &format!("{}.docx", stem(pdf)))
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

/// The PDF's name without `.pdf`, and without a `.docx` before it: a PDF
/// saved from `Name.docx` as `Name.docx.pdf` converts to `Name.docx`, not
/// `Name.docx.docx`.
fn stem(pdf: &Path) -> String {
    let stem = pdf
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".to_owned());
    match stem.len().checked_sub(".docx".len()) {
        Some(cut)
            if cut > 0
                && stem.is_char_boundary(cut)
                && stem[cut..].eq_ignore_ascii_case(".docx") =>
        {
            stem[..cut].to_owned()
        }
        _ => stem,
    }
}

/// Why a conversion failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConvertError {
    /// The system did not let the app save in the Word document's folder
    /// (macOS privacy protection of Downloads, Documents or Desktop,
    /// Windows' Controlled Folder Access, or a folder without write
    /// permission). Saving somewhere chosen in a Save panel works.
    NotPermitted(String),
    Failed(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPermitted(message) | Self::Failed(message) => f.write_str(message),
        }
    }
}

impl From<String> for ConvertError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

/// How far a conversion has got, for its progress bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvertStep {
    /// Reading the PDF's file and structure.
    Reading,
    /// `done` of `count` pages converted.
    Page { done: usize, count: usize },
    /// Writing the Word document.
    Writing,
}

/// Converts the PDF at `pdf` into a Word document at `docx`, replacing a
/// file there, telling `progress` each step. The document is written beside
/// it first and moved into place, so a failed conversion leaves any earlier
/// one as it was. Where the folder refuses a new file but `docx` itself may
/// be written (one chosen in a macOS Save panel), it is written in place.
/// When `progress` returns `false` the conversion stops before its next
/// page and saves nothing: `Ok(false)`.
pub fn convert(
    pdf: &Path,
    docx: &Path,
    progress: &mut dyn FnMut(ConvertStep) -> bool,
) -> Result<bool, ConvertError> {
    if !progress(ConvertStep::Reading) {
        return Ok(false);
    }
    let bytes =
        std::fs::read(pdf).map_err(|error| format!("The PDF could not be read: {error}"))?;
    // The reader is careful, but a PDF is outside data: a panic in it is a
    // failed conversion, not a crashed app.
    let converted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        docx_from_pdf_with_progress(&bytes, None, progress)
    }))
    .map_err(|_| "The PDF could not be converted to a Word document.".to_owned())??;
    let Some(converted) = converted else {
        return Ok(false);
    };
    let name = docx
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document.docx".to_owned());
    let partial = docx.with_file_name(format!(".{name}.part"));
    if let Err(error) = std::fs::write(&partial, &converted) {
        if error.kind() != std::io::ErrorKind::PermissionDenied {
            return Err(format!("The Word document could not be saved: {error}").into());
        }
        let _ = std::fs::remove_file(&partial);
        return write_in_place(docx, &converted).map(|()| true);
    }
    std::fs::rename(&partial, docx).map_err(|error| {
        let _ = std::fs::remove_file(&partial);
        format!(
            "The Word document could not be saved over {name}; close it in any app showing \
             it and try again ({error})."
        )
    })?;
    Ok(true)
}

/// Writes `bytes` straight to `docx`, removing what it wrote if that fails.
fn write_in_place(docx: &Path, bytes: &[u8]) -> Result<(), ConvertError> {
    std::fs::write(docx, bytes).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            ConvertError::NotPermitted(format!(
                "The Word document could not be saved in this folder: {error}"
            ))
        } else {
            let _ = std::fs::remove_file(docx);
            ConvertError::Failed(format!("The Word document could not be saved: {error}"))
        }
    })
}

/// The Word document for PDF `pdf`.
pub fn docx_from_pdf(pdf: &[u8]) -> Result<Vec<u8>, String> {
    docx_from_pdf_with_progress(pdf, None, &mut |_| true)
        .map(|converted| converted.expect("never stopped"))
}

/// The Word document of pages `pages` (counted from zero) of PDF `pdf`,
/// read as part of the whole document (fonts it shares between pages
/// included).
pub fn docx_from_pdf_pages(pdf: &[u8], pages: std::ops::Range<usize>) -> Result<Vec<u8>, String> {
    docx_from_pdf_with_progress(pdf, Some(pages), &mut |_| true)
        .map(|converted| converted.expect("never stopped"))
}

/// [`docx_from_pdf`], telling `progress` each page; `None` when it
/// returned `false` to stop.
fn docx_from_pdf_with_progress(
    pdf: &[u8],
    only: Option<std::ops::Range<usize>>,
    progress: &mut dyn FnMut(ConvertStep) -> bool,
) -> Result<Option<Vec<u8>>, String> {
    let document =
        Document::load_mem(pdf).map_err(|error| format!("The PDF could not be read: {error}"))?;
    if document.is_encrypted() {
        return Err("The PDF needs a password to open, so it cannot be converted.".into());
    }
    // Every page's object, found once: finding one page by its number
    // walks the whole page tree.
    let mut page_ids = document.get_pages().into_values().collect::<Vec<_>>();
    let mut first_page = 0;
    if let Some(only) = only {
        let end = only.end.min(page_ids.len());
        first_page = only.start.min(end);
        page_ids = page_ids[first_page..end].to_vec();
    }
    let count = page_ids.len();
    if !progress(ConvertStep::Page { done: 0, count }) {
        return Ok(None);
    }
    // Pages read on every core, as each is independent of the others.
    let mut pages: Vec<Option<Page>> = (0..count).map(|_| None).collect();
    let mut failed: Option<(usize, String)> = None;
    let mut done = 0;
    let mut stopped = false;
    let fonts = FontCache::default();
    in_parallel(
        &page_ids,
        |index, page_id| read_page(&document, (first_page + index) as u32, *page_id, &fonts),
        |index, page| {
            done += 1;
            match page {
                Ok(page) => pages[index] = Some(page),
                // Pages are taken in order, so every page before this one
                // is read too: the first failure is the first page's.
                Err(error) => {
                    if failed.as_ref().is_none_or(|(first, _)| index < *first) {
                        failed = Some((index, error));
                    }
                    return false;
                }
            }
            if done < count && !progress(ConvertStep::Page { done, count }) {
                stopped = true;
                return false;
            }
            true
        },
    );
    if let Some((_, error)) = failed {
        return Err(error);
    }
    if stopped || !progress(ConvertStep::Writing) {
        return Ok(None);
    }
    let mut pages = pages.into_iter().flatten().collect::<Vec<_>>();
    let mut media = embed_images(&document, &mut pages);
    let fallback = embed_svgs(&mut pages, &mut media);
    Ok(Some(write_docx(&pages, &media, fallback)))
}

/// Runs `work` on each of `items` across the machine's cores, taking them in
/// order, and hands each result to `done` on this thread as it finishes.
/// When `done` returns `false`, no further item starts.
fn in_parallel<T: Sync, R: Send>(
    items: &[T],
    work: impl Fn(usize, &T) -> R + Sync,
    mut done: impl FnMut(usize, R) -> bool,
) {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(items.len());
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let (sender, results) = std::sync::mpsc::channel();
        for _ in 0..threads {
            let (sender, next, stop, work) = (sender.clone(), &next, &stop, &work);
            scope.spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let index = next.fetch_add(1, Ordering::AcqRel);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    if sender.send((index, work(index, item))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
        for (index, result) in results {
            if !done(index, result) {
                stop.store(true, Ordering::Release);
            }
        }
    });
}

// --- The page model -------------------------------------------------------

/// A page as Word content.
struct Page {
    /// Left, bottom, right, top.
    page_box: [f64; 4],
    items: Vec<Item>,
    /// The page keeps the PDF's layout instead of flowing (`items` is then
    /// empty).
    fixed: Option<crate::docx_fixed::FixedPage>,
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
    /// The image XObject drawn.
    object: ObjectId,
    /// Into the document's media, once [`embed_images`] has encoded it.
    media: usize,
}

struct Media {
    bytes: Vec<u8>,
    extension: &'static str,
    /// The image's soft mask, when it is one opacity everywhere or opaque
    /// within a rectangle and clear outside it: drawn as the picture's
    /// opacity and crop instead of in its pixels.
    alpha: f64,
    crop: Option<[f64; 4]>,
}

fn read_page(
    document: &Document,
    index: u32,
    page_id: ObjectId,
    fonts: &FontCache,
) -> Result<Page, String> {
    let (layer, images) = extract_page_text_and_images_at(
        document,
        index,
        page_id,
        &TextEditLimits::default(),
        fonts,
    )
    .map_err(|error| format!("Page {} could not be read: {error:?}", index + 1))?;
    // Shared by each paragraph's reflow rather than copied for each.
    let layer = Arc::new(layer);
    let page_box = layer.page_box.unwrap_or([0., 0., 612., 792.]);
    let graphics =
        crate::pdf_page_graphics::page_graphics(document, page_id, page_box, &|object| {
            image_bytes(document, object)
        });
    if crate::docx_fixed::keeps_layout(&layer, &graphics) {
        let scale = crate::docx_fixed::page_scale(page_box);
        let lines = crate::docx_fixed::fixed_lines(&layer, page_box, &graphics, scale);
        return Ok(Page {
            page_box,
            items: Vec::new(),
            fixed: Some(crate::docx_fixed::FixedPage {
                scale,
                graphics,
                lines,
                svg_media: Vec::new(),
                picture_media: Vec::new(),
            }),
        });
    }
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
        let breaks = crate::pdf_text_reflow::Paragraph::new(layer.clone(), block)
            .map(|paragraph| paragraph.hard_breaks().to_vec())
            .unwrap_or_else(|| vec![false; lines.len()]);
        if let Some(para) = paragraph(&layer, &lines, &breaks, block.alignment, block.pitch) {
            place(para, Some(block), first);
        }
    }
    // Text Edit PDF cannot change still converts, a line at a time.
    for line in layer.lines.iter().filter(|line| {
        line.read_only.is_some()
            && !crate::docx_fixed::invisible(line)
            && !line.text.trim().is_empty()
    }) {
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
    items.extend(images.into_iter().filter_map(image_item));
    // Reading order: top to bottom, then left to right.
    items.sort_by(|a, b| {
        let (a, b) = (a.bounds(), b.bounds());
        b[3].total_cmp(&a[3]).then(a[0].total_cmp(&b[0]))
    });
    Ok(Page {
        page_box,
        items,
        fixed: None,
    })
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
        // A line has a few styles but many characters: each style once.
        let mut styles: Vec<Option<RunStyle>> = vec![None; line.styles.len()];
        let mut last_style = None;
        let mut buffer = [0; 4];
        for character in &line.chars[start..=end] {
            let style = styles[character.style]
                .get_or_insert_with(|| run_style(layer, line, character.style, underline));
            push(style, character.character.encode_utf8(&mut buffer));
            last_style = Some(character.style);
        }
        let last_style = last_style.and_then(|style| styles[style].clone());
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
pub(crate) fn font_family(
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

/// The image `image` draws, unless it is too small to matter.
fn image_item(image: PageImage) -> Option<Item> {
    let [left, bottom, right, top] = image.rect;
    if right - left < 4. || top - bottom < 4. {
        return None;
    }
    Some(Item::Image(Image {
        rect: image.rect,
        object: image.object,
        media: usize::MAX,
    }))
}

/// The media the document embeds for the images `pages` draw: a JPEG as it
/// is, anything else PNG, each image once however often it is drawn (a logo
/// on every page). Images the reader cannot decode are left out.
fn embed_images(document: &Document, pages: &mut [Page]) -> Vec<Media> {
    let mut objects = Vec::new();
    let mut slots = HashMap::new();
    for page in pages.iter() {
        for item in &page.items {
            if let Item::Image(image) = item {
                slots.entry((image.object, false)).or_insert_with(|| {
                    objects.push((image.object, false));
                    objects.len() - 1
                });
            }
        }
        for image in fixed_pictures(page) {
            slots
                .entry((image.object, image.over_paper))
                .or_insert_with(|| {
                    objects.push((image.object, image.over_paper));
                    objects.len() - 1
                });
        }
    }
    let mut encoded: Vec<Option<EncodedImage>> = (0..objects.len()).map(|_| None).collect();
    in_parallel(
        &objects,
        |_, (object, over_paper)| encode_image(document, *object, *over_paper),
        |index, bytes| {
            encoded[index] = bytes;
            true
        },
    );
    // Numbered in the order first drawn, without the ones left out.
    let mut media = Vec::new();
    let numbers = encoded
        .into_iter()
        .map(|bytes| {
            let encoded = bytes?;
            media.push(Media {
                bytes: encoded.bytes,
                extension: encoded.extension,
                alpha: encoded.alpha,
                crop: encoded.crop,
            });
            Some(media.len() - 1)
        })
        .collect::<Vec<_>>();
    for page in pages.iter_mut() {
        let pictures = fixed_pictures(page)
            .map(|image| numbers[slots[&(image.object, image.over_paper)]])
            .collect::<Vec<_>>();
        if let Some(fixed) = &mut page.fixed {
            // Pictures that cannot be encoded are left out.
            let mut kept = pictures.into_iter();
            fixed.graphics.layers.retain(|layer| match layer {
                crate::pdf_page_graphics::GraphicsLayer::Picture(_) => {
                    kept.next().flatten().is_some()
                }
                crate::pdf_page_graphics::GraphicsLayer::Svg(_) => true,
            });
            fixed.picture_media = fixed
                .graphics
                .layers
                .iter()
                .filter_map(|layer| match layer {
                    crate::pdf_page_graphics::GraphicsLayer::Picture(image) => {
                        numbers[slots[&(image.object, image.over_paper)]]
                    }
                    crate::pdf_page_graphics::GraphicsLayer::Svg(_) => None,
                })
                .collect();
        }
        page.items.retain_mut(|item| match item {
            Item::Image(image) => match numbers[slots[&(image.object, false)]] {
                Some(number) => {
                    image.media = number;
                    true
                }
                None => false,
            },
            _ => true,
        });
    }
    media
}

/// The pictures a fixed page places, in layer order.
fn fixed_pictures(page: &Page) -> impl Iterator<Item = &crate::pdf_page_graphics::PlacedImage> {
    page.fixed.iter().flat_map(|fixed| {
        fixed
            .graphics
            .layers
            .iter()
            .filter_map(|layer| match layer {
                crate::pdf_page_graphics::GraphicsLayer::Picture(image) => Some(image),
                crate::pdf_page_graphics::GraphicsLayer::Svg(_) => None,
            })
    })
}

/// Adds each fixed page's SVG layers to the document's media, moving them
/// out of the pages, and the picture shown where SVG is not drawn.
fn embed_svgs(pages: &mut [Page], media: &mut Vec<Media>) -> Option<usize> {
    let mut fallback = None;
    for page in pages {
        let Some(fixed) = &mut page.fixed else {
            continue;
        };
        let [left, bottom, right, top] = page.page_box;
        let size = ((right - left) * fixed.scale, (top - bottom) * fixed.scale);
        for layer in &mut fixed.graphics.layers {
            if let crate::pdf_page_graphics::GraphicsLayer::Svg(svg) = layer {
                // Its own size is the picture's: LibreOffice draws an SVG
                // shown at another size as a blurred bitmap.
                let svg = crate::pdf_page_graphics::with_display_size(std::mem::take(svg), size);
                media.push(Media {
                    bytes: svg.into_bytes(),
                    extension: "svg",
                    alpha: 1.,
                    crop: None,
                });
                fixed.svg_media.push(media.len() - 1);
                if fallback.is_none() {
                    media.push(Media {
                        bytes: TRANSPARENT_PNG.to_vec(),
                        extension: "png",
                        alpha: 1.,
                        crop: None,
                    });
                    fallback = Some(media.len() - 1);
                }
            }
        }
    }
    fallback
}

/// A transparent pixel: what a word processor without SVG shows for a
/// drawing layer.
const TRANSPARENT_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

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

/// An image as the document embeds it.
struct EncodedImage {
    bytes: Vec<u8>,
    extension: &'static str,
    alpha: f64,
    crop: Option<[f64; 4]>,
}

/// What an image's soft mask does.
enum MaskEffect {
    /// Nothing: it is opaque everywhere.
    Opaque,
    /// One opacity everywhere.
    Uniform(f64),
    /// Opaque within a rectangle (fractions cut from the left, top, right
    /// and bottom) and clear outside it.
    Crop([f64; 4]),
    /// Anything else, drawn in the picture's own alpha.
    Soft,
}

fn mask_effect(document: &Document, mask: &Object) -> MaskEffect {
    let Some(stream) = resolved(document, mask).and_then(|mask| mask.as_stream().ok()) else {
        return MaskEffect::Opaque;
    };
    let dimension = |key: &[u8]| {
        stream
            .dict
            .get(key)
            .ok()
            .and_then(|value| resolved(document, value))
            .and_then(|value| value.as_i64().ok())
    };
    let (Some(width), Some(height)) = (dimension(b"Width"), dimension(b"Height")) else {
        return MaskEffect::Soft;
    };
    let bits = dimension(b"BitsPerComponent").unwrap_or(8);
    let Ok(alpha) = stream.decompressed_content() else {
        return MaskEffect::Soft;
    };
    let (width, height) = (width.max(1) as usize, height.max(1) as usize);
    if bits != 8 || alpha.len() < width * height {
        return MaskEffect::Soft;
    }
    let alpha = &alpha[..width * height];
    let first = alpha[0];
    if alpha.iter().all(|value| *value == first) {
        return match first {
            255 => MaskEffect::Opaque,
            value => MaskEffect::Uniform(f64::from(value) / 255.),
        };
    }
    // Opaque inside its bounds and clear outside: a crop.
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for y in 0..height {
        let row = &alpha[y * width..(y + 1) * width];
        for (x, value) in row.iter().enumerate() {
            match value {
                255 => {
                    left = left.min(x);
                    right = right.max(x + 1);
                    top = top.min(y);
                    bottom = bottom.max(y + 1);
                }
                0 => {}
                _ => return MaskEffect::Soft,
            }
        }
    }
    if left >= right || top >= bottom {
        return MaskEffect::Soft;
    }
    let inside_opaque = (top..bottom).all(|y| {
        alpha[y * width + left..y * width + right]
            .iter()
            .all(|v| *v == 255)
    });
    if !inside_opaque {
        return MaskEffect::Soft;
    }
    MaskEffect::Crop([
        left as f64 / width as f64,
        top as f64 / height as f64,
        (width - right) as f64 / width as f64,
        (height - bottom) as f64 / height as f64,
    ])
}

/// An image as JPEG (as it is) or PNG, its pixels alone: a soft mask that
/// is a rectangle is left for the picture's crop to draw, and one that is
/// one opacity is drawn over white for an image `over_paper` (nothing
/// drawn beneath it, so it stays a JPEG) and into the alpha of a PNG
/// otherwise.
fn encode_image(document: &Document, object: ObjectId, over_paper: bool) -> Option<EncodedImage> {
    let stream = document.get_object(object).ok()?.as_stream().ok()?;
    let effect = match stream.dict.get(b"SMask") {
        Ok(mask) => mask_effect(document, mask),
        Err(_) => MaskEffect::Opaque,
    };
    let (alpha, crop) = match effect {
        MaskEffect::Opaque => (1., None),
        MaskEffect::Uniform(alpha) => (alpha, None),
        MaskEffect::Crop(crop) => (1., Some(crop)),
        MaskEffect::Soft => {
            let (bytes, extension) = image_bytes(document, object)?;
            return Some(EncodedImage {
                bytes,
                extension,
                alpha: 1.,
                crop: None,
            });
        }
    };
    if alpha <= 0. {
        return None;
    }
    let (bytes, extension) = image_pixels(document, object, false)?;
    // Over other drawing, one opacity goes into the picture's alpha: every
    // word processor draws that, where LibreOffice ignores a picture's own
    // opacity.
    if !over_paper && alpha < 1. {
        let mut rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let opacity = (alpha * 255.).round() as u8;
        for pixel in rgba.pixels_mut() {
            pixel.0[3] = opacity;
        }
        let mut png = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new(&mut png),
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
        return Some(EncodedImage {
            bytes: png,
            extension: "png",
            alpha: 1.,
            crop,
        });
    }
    if over_paper && alpha < 1. {
        let decoded = image::load_from_memory(&bytes).ok()?.to_rgb8();
        let mut faded = decoded;
        for pixel in faded.pixels_mut() {
            for channel in &mut pixel.0 {
                *channel = (255. - alpha * (255. - f64::from(*channel))).round() as u8;
            }
        }
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode_image(&faded)
            .ok()?;
        return Some(EncodedImage {
            bytes: jpeg,
            extension: "jpeg",
            alpha: 1.,
            crop,
        });
    }
    Some(EncodedImage {
        bytes,
        extension,
        alpha,
        crop,
    })
}

/// An image as JPEG or PNG, its soft mask drawn into its alpha.
fn image_bytes(document: &Document, object: ObjectId) -> Option<(Vec<u8>, &'static str)> {
    image_pixels(document, object, true)
}

fn image_pixels(
    document: &Document,
    object: ObjectId,
    with_mask: bool,
) -> Option<(Vec<u8>, &'static str)> {
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
    let has_mask = with_mask && dict.get(b"SMask").is_ok();
    let filters = filters(dict, document);
    // A soft mask that covers nothing (all opaque) changes nothing.
    let has_mask = has_mask
        && !resolved(document, dict.get(b"SMask").ok()?)
            .and_then(|mask| mask.as_stream().ok())
            .and_then(|mask| mask.decompressed_content().ok())
            .is_some_and(|alpha| !alpha.is_empty() && alpha.iter().all(|value| *value == 255));
    // A plain JPEG embeds as it is, once out of any compression around it
    // (PDF24 deflates its JPEGs).
    if filters.last().is_some_and(|filter| filter == b"DCTDecode")
        && matches!(components, 1 | 3)
        && !has_mask
    {
        if filters.len() == 1 {
            return Some((stream.content.clone(), "jpeg"));
        }
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
        if let Ok(jpeg) = copy.decompressed_content() {
            return Some((jpeg, "jpeg"));
        }
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
pub(crate) fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    escape_into(text, &mut escaped);
    escaped
}

/// [`escape`], written onto the end of `out`.
pub(crate) fn escape_into(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(character),
            character if (character as u32) < 0x20 => {}
            '\u{fffe}' | '\u{ffff}' => {}
            character => out.push(character),
        }
    }
}

// The XML is written straight onto the document's: formatting each piece
// as a string of its own first took most of the time spent writing it.
fn run_xml(run: &Run, out: &mut String) {
    let style = &run.style;
    let font = escape(&style.font);
    let _ = write!(
        out,
        "<w:r><w:rPr><w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:cs=\"{font}\" w:eastAsia=\"{font}\"/>"
    );
    if style.bold {
        out.push_str("<w:b/><w:bCs/>");
    }
    if style.italic {
        out.push_str("<w:i/><w:iCs/>");
    }
    if style.underline {
        out.push_str("<w:u w:val=\"single\"/>");
    }
    if let Some([r, g, b]) = style.colour {
        let _ = write!(out, "<w:color w:val=\"{r:02X}{g:02X}{b:02X}\"/>");
    }
    let half_points = (style.size * 2.).round().max(2.) as i64;
    let _ = write!(
        out,
        "<w:sz w:val=\"{half_points}\"/><w:szCs w:val=\"{half_points}\"/></w:rPr>"
    );
    for (index, piece) in run.text.split('\n').enumerate() {
        if index > 0 {
            out.push_str("<w:br/>");
        }
        for (index, part) in piece.split('\t').enumerate() {
            if index > 0 {
                out.push_str("<w:tab/>");
            }
            if !part.is_empty() {
                out.push_str("<w:t xml:space=\"preserve\">");
                escape_into(part, out);
                out.push_str("</w:t>");
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
    let _ = write!(
        out,
        "<w:p><w:pPr><w:spacing w:before=\"{}\" w:after=\"0\"",
        twips(before.max(0.))
    );
    if let Some(pitch) = para.pitch {
        let _ = write!(out, " w:line=\"{}\" w:lineRule=\"atLeast\"", twips(pitch));
    }
    out.push_str("/>");
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
            let _ = write!(out, "<w:ind w:left=\"{}\"", twips(left));
            let _ = if first >= 0. {
                write!(out, " w:firstLine=\"{}\"/>", twips(first))
            } else {
                write!(out, " w:hanging=\"{}\"/>", twips(-first))
            };
        }
    }
    let justification = match alignment {
        TextBlockAlignment::Left => "left",
        TextBlockAlignment::Right => "right",
        TextBlockAlignment::Center => "center",
        TextBlockAlignment::Justified => "both",
    };
    let _ = write!(out, "<w:jc w:val=\"{justification}\"/>");
    if let Some(section) = section {
        out.push_str(section);
    }
    out.push_str("</w:pPr>");
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
    let _ = write!(
        out,
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
    );
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
    let _ = write!(
        out,
        "<w:tbl><w:tblPr><w:tblW w:w=\"{}\" w:type=\"dxa\"/><w:tblInd w:w=\"{}\" w:type=\"dxa\"/>\
         <w:tblBorders>{borders}</w:tblBorders><w:tblLayout w:type=\"fixed\"/>\
         <w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"{2}\" w:type=\"dxa\"/>\
         <w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"{2}\" w:type=\"dxa\"/></w:tblCellMar>\
         <w:tblLook w:val=\"0000\"/></w:tblPr><w:tblGrid>",
        widths.iter().sum::<i64>(),
        twips(table.columns[0] - area_left),
        twips(inset),
    );
    for width in &widths {
        let _ = write!(out, "<w:gridCol w:w=\"{width}\"/>");
    }
    out.push_str("</w:tblGrid>");
    let row_count = table.rows.len() - 1;
    let column_count = table.columns.len() - 1;
    for row in 0..row_count {
        let _ = write!(
            out,
            "<w:tr><w:trPr><w:trHeight w:val=\"{}\" w:hRule=\"atLeast\"/></w:trPr>",
            twips(table.rows[row] - table.rows[row + 1])
        );
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
            let _ = write!(
                out,
                "<w:tc><w:tcPr><w:tcW w:w=\"{width}\" w:type=\"dxa\"/>{grid_span}{merge}</w:tcPr>"
            );
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
    let scale = page.fixed.as_ref().map_or(1., |fixed| fixed.scale);
    let [left, bottom, right, top] = page.page_box;
    let (width, height) = ((right - left) * scale, (top - bottom) * scale);
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

fn document_xml(
    pages: &[Page],
    media_relationships: &[String],
    media: &[Media],
    fallback: Option<usize>,
) -> String {
    // Pictures are numbered through the document: each page's first one.
    let first_pictures = pages
        .iter()
        .scan(0, |pictures, page| {
            let first = *pictures;
            *pictures += page
                .items
                .iter()
                .filter(|item| matches!(item, Item::Image(_)))
                .count();
            Some(first)
        })
        .collect::<Vec<_>>();
    // Each page's XML is its own, so the pages are written on every core.
    let mut bodies = vec![String::new(); pages.len()];
    in_parallel(
        pages,
        |index, page| {
            let mut body = String::new();
            if let Some(fixed) = &page.fixed {
                let svg = fixed
                    .svg_media
                    .iter()
                    .map(|media| media_relationships[*media].clone())
                    .collect::<Vec<_>>();
                let pictures = fixed
                    .picture_media
                    .iter()
                    .map(|index| crate::docx_fixed::PictureMedia {
                        relationship: media_relationships[*index].clone(),
                        alpha: media[*index].alpha,
                        crop: media[*index].crop,
                    })
                    .collect::<Vec<_>>();
                let fallback = fallback.map_or("", |media| media_relationships[media].as_str());
                let last = index + 1 == pages.len();
                let section = section_xml(page, [0.; 4]);
                crate::docx_fixed::fixed_page_xml(
                    fixed,
                    page.page_box,
                    &crate::docx_fixed::FixedMedia {
                        svg: &svg,
                        pictures: &pictures,
                        fallback,
                    },
                    // Ids apart from the flowing pages' pictures.
                    1_000_000 + 100_000 * index,
                    (!last).then_some(section.as_str()),
                    &mut body,
                );
                if last {
                    body.push_str(&section);
                }
                return body;
            }
            page_xml(
                page,
                index + 1 == pages.len(),
                first_pictures[index],
                media_relationships,
                &mut body,
            );
            body
        },
        |index, body| {
            bodies[index] = body;
            true
        },
    );
    const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" \
         xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">\
         <w:body>";
    const TAIL: &str = "</w:body></w:document>";
    let mut xml = String::with_capacity(
        HEAD.len() + bodies.iter().map(String::len).sum::<usize>() + TAIL.len(),
    );
    xml.push_str(HEAD);
    for body in &bodies {
        xml.push_str(body);
    }
    xml.push_str(TAIL);
    xml
}

/// The body XML of `page`, whose first picture follows `picture` others.
fn page_xml(
    page: &Page,
    last_page: bool,
    mut picture: usize,
    media_relationships: &[String],
    body: &mut String,
) {
    let margins = margins(page);
    let area_left = page.page_box[0] + margins[0];
    let area_right = page.page_box[2] - margins[2];
    let section = section_xml(page, margins);
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
                body,
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
                    body,
                )
            }
            Item::Table(table) => {
                if before > 1. {
                    // The gap above a table, as an empty paragraph's
                    // spacing.
                    let _ = write!(
                        body,
                        "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"{}\" w:lineRule=\"exact\"/></w:pPr></w:p>",
                        twips(before.max(1.))
                    );
                }
                table_xml(table, area_left, body);
                if let Some(section) = ends_section(item_index) {
                    let _ = write!(
                        body,
                        "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>{section}</w:pPr></w:p>"
                    );
                }
            }
        }
        previous = item.bounds()[1];
    }
    if page.items.is_empty() && !last_page {
        let _ = write!(body, "<w:p><w:pPr>{section}</w:pPr></w:p>");
    }
    if last_page {
        if matches!(page.items.last(), Some(Item::Table(_))) {
            // A table cannot end the document.
            body.push_str("<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/></w:pPr></w:p>");
        }
        body.push_str(&section);
    }
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Default Extension=\"png\" ContentType=\"image/png\"/>\
<Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/>\
<Default Extension=\"svg\" ContentType=\"image/svg+xml\"/>\
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

fn write_docx(pages: &[Page], media: &[Media], fallback: Option<usize>) -> Vec<u8> {
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
        let _ = write!(
            document_relationships,
            "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{}.{}\"/>",
            relationships[index],
            index + 1,
            item.extension
        );
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
            document_xml(pages, &relationships, media, fallback).into_bytes(),
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
        // Images are compressed already; SVG is text.
        entries.push((
            format!("word/media/image{}.{}", index + 1, item.extension),
            item.bytes.clone(),
            item.extension == "svg",
        ));
    }
    zip(&entries)
}

// --- Zip -----------------------------------------------------------------

/// `contents` deflated in pieces on every core and joined into one stream,
/// as `pigz` does: each piece but the last ends with a sync flush, on a
/// byte boundary without ending the stream, so the pieces join as they
/// are. Level 3 takes half the default's time for a document an eighth
/// larger (Word compresses it again when it saves).
fn deflate_in_parallel(contents: &[u8]) -> Vec<u8> {
    const PIECE: usize = 1 << 20;
    let pieces = contents.chunks(PIECE).collect::<Vec<_>>();
    let last = pieces.len().saturating_sub(1);
    let mut deflated = vec![Vec::new(); pieces.len().max(1)];
    let deflate = |index: usize, piece: &[u8]| {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(3));
        encoder.write_all(piece).expect("writing to memory");
        if index == last {
            encoder.finish().expect("writing to memory")
        } else {
            encoder.flush().expect("writing to memory");
            std::mem::take(encoder.get_mut())
        }
    };
    if pieces.is_empty() {
        deflated[0] = deflate(0, &[]);
    }
    in_parallel(
        &pieces,
        |index, piece| deflate(index, piece),
        |index, bytes| {
            deflated[index] = bytes;
            true
        },
    );
    deflated.concat()
}

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
            (8u16, deflate_in_parallel(contents))
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

    /// Pieces deflated apart join into one stream that inflates to the
    /// whole, whether it is several pieces, one or empty.
    #[test]
    fn pieces_deflated_on_every_core_join_into_one_stream() {
        use std::io::Read as _;
        let text = (0..200_000)
            .map(|index| format!("<w:t>{}</w:t>", index * 7919 % 100_003))
            .collect::<String>();
        for contents in [text.as_bytes(), b"<w:body/>".as_slice(), b"".as_slice()] {
            let mut inflated = Vec::new();
            flate2::read::DeflateDecoder::new(deflate_in_parallel(contents).as_slice())
                .read_to_end(&mut inflated)
                .unwrap();
            assert_eq!(inflated, contents);
        }
        assert!(text.len() > 3 << 20, "several pieces");
    }

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
