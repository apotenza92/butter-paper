//! A local PDF corpus check, run by `cargo xtask pdf-corpus`: for each listed
//! PDF it compares the editable text with PDFium's characters, checks that
//! the outlines Edit PDF Content draws sit on rendered ink, round-trips edits, and
//! times each step. It reads the PDFs and writes only under its output
//! folder; it never runs in CI.

use super::tests::{source_handle, test_backend};
use super::*;
use butter_paper::annotation_model::{PageTransform, PdfPoint};
use butter_paper::page_geometry::{PdfRect, Rotation as SpaceRotation};
use butter_paper::page_text_editor::PageTextEditState;
use butter_paper::pdf_text_blocks::{PageTextBlock, paragraph_blocks};
use butter_paper::pdf_text_edit::{PageTextEdit, PageTextLayer, PageTextLine};
use butter_paper::pdf_text_reflow::Paragraph;
use butter_paper::pdf_worker::{
    ClipRect, JobId, RenderRequest, SessionId, SurfaceDescriptor, SurfaceFormat, SurfaceId,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// How far (points) PDFium's character origin may be from ours.
const ALIGN_TOLERANCE: f64 = 1.5;
/// A page is misaligned when fewer of its checked characters match.
const ALIGNED_SHARE: f64 = 0.9;
/// Rendering resolution cap, in pixels per page.
const MAX_RENDER_PIXELS: f64 = 2_500_000.;
/// How far (0-255, any channel) a pixel must differ from its box's
/// background to be ink.
const INK_CONTRAST: u8 = 40;
/// Text typed into each round-tripped paragraph.
const TYPED: &str = " zqx";

#[derive(Default, Serialize)]
struct DocReport {
    path: String,
    bytes: u64,
    pages: usize,
    sampled: usize,
    error: Option<String>,
    panics: Vec<String>,
    load_ms: f64,
    extract_ms: f64,
    extract_max_ms: f64,
    extract_max_page: usize,
    pdfium_ms: f64,
    lines: usize,
    editable_lines: usize,
    read_only: BTreeMap<String, usize>,
    blocks: usize,
    single_line_blocks: usize,
    fragment_lines: usize,
    checked_chars: usize,
    aligned_chars: usize,
    misaligned_pages: Vec<PageIssue>,
    ink_lines: usize,
    inkless_lines: usize,
    inkless_pages: Vec<PageIssue>,
    edits_tried: usize,
    edit_failures: Vec<String>,
    noop_changed: usize,
    apply_max_ms: f64,
    worker_edit_ms: Option<f64>,
    crops: Vec<String>,
}

#[derive(Serialize)]
struct PageIssue {
    page: usize,
    detail: String,
}

struct Extracted {
    page: usize,
    layer: Result<Arc<PageTextLayer>, String>,
    blocks: Vec<PageTextBlock>,
    ms: f64,
    panic: Option<String>,
}

#[test]
#[ignore = "local corpus check, run by `cargo xtask pdf-corpus`"]
fn pdf_corpus() {
    let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
    let list = PathBuf::from(env::var_os("BP_CORPUS_LIST").expect("BP_CORPUS_LIST"));
    let out = PathBuf::from(env::var_os("BP_CORPUS_OUT").expect("BP_CORPUS_OUT"));
    let max_pages = env_number("BP_CORPUS_PAGES", 30);
    let max_edits = env_number("BP_CORPUS_EDITS", 3);
    let threads = env_number(
        "BP_CORPUS_THREADS",
        std::thread::available_parallelism().map_or(4, usize::from),
    )
    .max(1);
    let crops_dir = out.join("crops");
    std::fs::create_dir_all(&crops_dir).unwrap();
    let paths = std::fs::read_to_string(&list).unwrap();
    let results = out.join(format!(
        "{}.jsonl",
        list.file_stem().unwrap().to_string_lossy()
    ));
    let mut results = std::fs::File::create(results).unwrap();
    let (_guard, mut backend) = test_backend(Path::new(&library));
    // Problems are reported, not printed per page.
    std::panic::set_hook(Box::new(|_| {}));
    for path in paths.lines().filter(|line| !line.is_empty()) {
        let started = Instant::now();
        let report = catch_unwind(AssertUnwindSafe(|| {
            check_document(
                &mut backend,
                Path::new(path),
                max_pages,
                max_edits,
                threads,
                &crops_dir,
            )
        }))
        .unwrap_or_else(|panic| DocReport {
            path: path.to_owned(),
            error: Some(format!("harness panic: {}", panic_text(&panic))),
            ..DocReport::default()
        });
        writeln!(results, "{}", serde_json::to_string(&report).unwrap()).unwrap();
        eprintln!(
            "{:>7.0} ms  {}{}",
            started.elapsed().as_secs_f64() * 1000.,
            path,
            report
                .error
                .as_deref()
                .map(|error| format!("  ({error})"))
                .unwrap_or_default()
        );
    }
}

fn env_number(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn panic_text(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
        .unwrap_or_else(|| "panic".into())
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.
}

/// Up to `max` page indices spread evenly through `count` pages; all when
/// `max` is zero.
fn sample(count: usize, max: usize) -> Vec<usize> {
    if max == 0 || count <= max {
        return (0..count).collect();
    }
    (0..max)
        .map(|index| index * (count - 1) / (max - 1))
        .collect()
}

fn check_document(
    backend: &mut PdfiumBackend,
    path: &Path,
    max_pages: usize,
    max_edits: usize,
    threads: usize,
    crops_dir: &Path,
) -> DocReport {
    let mut report = DocReport {
        path: path.display().to_string(),
        ..DocReport::default()
    };
    let started = Instant::now();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            report.error = Some(format!("read: {error}"));
            return report;
        }
    };
    report.bytes = bytes.len() as u64;
    let document = match PdfMetadataDocument::load_mem(&bytes) {
        Ok(document) => document,
        Err(error) => {
            report.error = Some(format!("lopdf: {error}"));
            return report;
        }
    };
    report.load_ms = ms(started);
    let pages = document.get_pages();
    report.pages = pages.len();
    let sampled = sample(pages.len(), max_pages);
    report.sampled = sampled.len();

    // Extraction and grouping on every core: lopdf reads are shared.
    let started = Instant::now();
    let extracted = extract_pages(&document, &sampled, threads);
    report.extract_ms = ms(started);
    for page in &extracted {
        if page.ms > report.extract_max_ms {
            report.extract_max_ms = page.ms;
            report.extract_max_page = page.page;
        }
        if let Some(panic) = &page.panic {
            report.panics.push(format!("page {}: {panic}", page.page));
        }
        let Ok(layer) = &page.layer else {
            continue;
        };
        for line in &layer.lines {
            if line.text.trim().is_empty() {
                continue;
            }
            report.lines += 1;
            match &line.read_only {
                Some(reason) => *report.read_only.entry(reason.clone()).or_default() += 1,
                None => {
                    report.editable_lines += 1;
                    if line.text.trim().chars().count() <= 2 {
                        report.fragment_lines += 1;
                    }
                }
            }
        }
        report.blocks += page.blocks.len();
        report.single_line_blocks += page
            .blocks
            .iter()
            .filter(|block| block.lines.len() == 1)
            .count();
    }

    // PDFium: alignment and ink, one page at a time.
    let started = Instant::now();
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            report.error = Some(format!("open: {error}"));
            return report;
        }
    };
    let mut pdfium = match backend.open(source_handle(&file), None) {
        Ok((pdfium, _)) => pdfium,
        Err(error) => {
            report.error = Some(format!("pdfium: {error:?}"));
            return report;
        }
    };
    for page in &extracted {
        let Ok(layer) = &page.layer else {
            continue;
        };
        if let Err(error) = check_page_pdfium(
            backend,
            &mut pdfium,
            &document,
            page.page,
            layer,
            &mut report,
            crops_dir,
        ) {
            report
                .panics
                .push(format!("page {} pdfium: {error}", page.page));
        }
    }
    report.pdfium_ms = ms(started);

    // Edits on copies, a few paragraphs spread through the document.
    let candidates = extracted
        .iter()
        .filter_map(|page| page.layer.as_ref().ok().map(|layer| (page, layer)))
        .flat_map(|(page, layer)| page.blocks.iter().map(move |block| (layer, block)))
        .filter(|(_, block)| !block.lines.is_empty())
        // Edit PDF Content offers only what the page shows.
        .filter(|(layer, block)| {
            let Some([x0, y0, x1, y1]) = layer.page_box else {
                return true;
            };
            block.lines.iter().all(|anchor| {
                layer.line(anchor).is_some_and(|line| {
                    let (x, y) = line.origin();
                    (x0..=x1).contains(&x) && (y0..=y1).contains(&y)
                })
            })
        })
        .collect::<Vec<_>>();
    let picks = sample(candidates.len(), max_edits.min(candidates.len()))
        .into_iter()
        .take(max_edits)
        .collect::<Vec<_>>();
    for (index, pick) in picks.iter().enumerate() {
        let (layer, block) = candidates[*pick];
        report.edits_tried += 1;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            round_trip(&document, layer, block, &mut report).inspect_err(|_| {
                keep_failure(
                    &document,
                    &state_edits(layer, block),
                    path,
                    layer.page_index,
                    crops_dir,
                )
            })
        }));
        match outcome {
            Ok(Ok(edits)) if index == 0 => {
                // The worker's own path: apply, re-render, read back.
                let started = Instant::now();
                // PDFium reads text in position order, so typed text that
                // runs up to other text interleaves with it: count the typed
                // letters instead.
                let typed_count = |pdfium: &PdfiumDocument| {
                    pdfium
                        .document
                        .pages()
                        .get(layer.page_index as i32)
                        // Every character: `all()` reads only a page-sized
                        // rectangle at the origin, missing text on pages
                        // whose box starts elsewhere.
                        .and_then(|page| {
                            page.text().map(|text| {
                                text.chars()
                                    .iter()
                                    .filter_map(|character| character.unicode_char())
                                    .collect::<String>()
                            })
                        })
                        .map(|text| {
                            TYPED
                                .trim()
                                .chars()
                                .map(|letter| text.matches(letter).count())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                };
                let before = typed_count(&pdfium);
                match backend.set_document_edits(&mut pdfium, &DocumentEdits::from_text(&edits)) {
                    Ok(()) => {
                        report.worker_edit_ms = Some(ms(started));
                        let after = typed_count(&pdfium);
                        let read_back = after.len() == before.len()
                            && after
                                .iter()
                                .zip(&before)
                                .all(|(after, before)| after > before);
                        if !read_back {
                            report.edit_failures.push(format!(
                                "page {}: PDFium does not read back every typed letter",
                                layer.page_index
                            ));
                            keep_failure(&document, &edits, path, layer.page_index, crops_dir);
                        }
                        let _ = backend.set_document_edits(&mut pdfium, &DocumentEdits::default());
                    }
                    Err(error) => report.edit_failures.push(format!(
                        "page {}: worker edit failed: {error:?}",
                        layer.page_index
                    )),
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(failure)) => report
                .edit_failures
                .push(format!("page {}: {failure}", layer.page_index)),
            Err(panic) => report.panics.push(format!(
                "page {} edit: {}",
                layer.page_index,
                panic_text(&panic)
            )),
        }
    }
    report
}

fn extract_pages(
    document: &PdfMetadataDocument,
    pages: &[usize],
    threads: usize,
) -> Vec<Extracted> {
    let chunk = pages.len().div_ceil(threads).max(1);
    let mut extracted = std::thread::scope(|scope| {
        let workers = pages
            .chunks(chunk)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|&page| extract_page(document, page))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .collect::<Vec<_>>()
    });
    extracted.sort_by_key(|page| page.page);
    extracted
}

fn extract_page(document: &PdfMetadataDocument, page: usize) -> Extracted {
    let started = Instant::now();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let layer = extract_page_text(document, page as u32, &TextEditLimits::default())
            .map(Arc::new)
            .map_err(|error| error.to_string())?;
        let blocks = paragraph_blocks(&layer);
        // Every paragraph must open in the editor.
        for block in &blocks {
            if let Some(paragraph) = Paragraph::new(layer.clone(), block) {
                let _ = PageTextEditState::new(paragraph, &[], &[], &[], 0);
            }
        }
        Ok::<_, String>((layer, blocks))
    }));
    let elapsed = ms(started);
    match outcome {
        Ok(Ok((layer, blocks))) => Extracted {
            page,
            layer: Ok(layer),
            blocks,
            ms: elapsed,
            panic: None,
        },
        Ok(Err(error)) => Extracted {
            page,
            layer: Err(error),
            blocks: Vec::new(),
            ms: elapsed,
            panic: None,
        },
        Err(panic) => Extracted {
            page,
            layer: Err("panic".into()),
            blocks: Vec::new(),
            ms: elapsed,
            panic: Some(panic_text(&panic)),
        },
    }
}

/// Where a character starts on its baseline, in page space.
fn baseline_point(line: &PageTextLine, x: f64) -> (f64, f64) {
    let quad = line.quad(x, x);
    let height = line.ascent - line.descent;
    let t = if height.abs() < 1e-9 {
        0.
    } else {
        -line.descent / height
    };
    (
        quad[0].0 + (quad[3].0 - quad[0].0) * t,
        quad[0].1 + (quad[3].1 - quad[0].1) * t,
    )
}

/// The page's crop box, rotation and user unit as the app reads them.
fn page_space(document: &PdfMetadataDocument, page: usize) -> Option<PageCoordinateSpace> {
    let id = *document.get_pages().get(&(page as u32 + 1))?;
    let inherited = |key: &[u8]| {
        let mut current = document.get_dictionary(id).ok()?;
        for _ in 0..32 {
            if let Ok(value) = current.get(key) {
                return Some(value.clone());
            }
            let parent = current.get(b"Parent").ok()?.as_reference().ok()?;
            current = document.get_dictionary(parent).ok()?;
        }
        None
    };
    let rect = |key: &[u8]| -> Option<PdfRect> {
        let array = inherited(key)?;
        let array = match array {
            lopdf::Object::Reference(id) => document.get_object(id).ok()?.clone(),
            other => other,
        };
        let values = array
            .as_array()
            .ok()?
            .iter()
            .filter_map(|value| {
                value
                    .as_float()
                    .ok()
                    .map(f64::from)
                    .or(value.as_i64().ok().map(|v| v as f64))
            })
            .collect::<Vec<_>>();
        let [a, b, c, d] = values[..] else {
            return None;
        };
        PdfRect::new(a.min(c), b.min(d), (c - a).abs(), (d - b).abs()).ok()
    };
    let media = rect(b"MediaBox")?;
    let crop = rect(b"CropBox").unwrap_or(media);
    let rotation = match inherited(b"Rotate")
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360)
    {
        90 => SpaceRotation::Degrees90,
        180 => SpaceRotation::Degrees180,
        270 => SpaceRotation::Degrees270,
        _ => SpaceRotation::Degrees0,
    };
    let user_unit = inherited(b"UserUnit")
        .and_then(|value| value.as_float().ok().map(f64::from))
        .unwrap_or(1.);
    PageCoordinateSpace::new(media, crop, rotation, user_unit).ok()
}

fn check_page_pdfium(
    backend: &mut PdfiumBackend,
    pdfium: &mut PdfiumDocument,
    document: &PdfMetadataDocument,
    page_index: usize,
    layer: &PageTextLayer,
    report: &mut DocReport,
    crops_dir: &Path,
) -> Result<(), String> {
    // PDFium's characters by kind and 4-point cell.
    let mut grid: HashMap<(char, i64, i64), Vec<(f64, f64)>> = HashMap::new();
    let mut origins = Vec::new();
    {
        let page = pdfium
            .document
            .pages()
            .get(page_index as i32)
            .map_err(|error| format!("{error:?}"))?;
        let text = page.text().map_err(|error| format!("{error:?}"))?;
        for character in text.chars().iter() {
            let (Some(unicode), Ok((x, y))) = (character.unicode_char(), character.origin()) else {
                continue;
            };
            if unicode.is_whitespace() {
                continue;
            }
            let (x, y) = (f64::from(x.value), f64::from(y.value));
            grid.entry((unicode, (x / 4.).floor() as i64, (y / 4.).floor() as i64))
                .or_default()
                .push((x, y));
            origins.push((x, y));
        }
    }
    let near = |unicode: char, (x, y): (f64, f64), reach: f64| -> Option<(f64, f64)> {
        let cells = (reach / 4.).ceil() as i64;
        let (cx, cy) = ((x / 4.).floor() as i64, (y / 4.).floor() as i64);
        let mut best: Option<(f64, (f64, f64))> = None;
        for dx in -cells..=cells {
            for dy in -cells..=cells {
                for &(px, py) in grid.get(&(unicode, cx + dx, cy + dy)).into_iter().flatten() {
                    let distance = (px - x).hypot(py - y);
                    if distance <= reach && best.is_none_or(|(best, _)| distance < best) {
                        best = Some((distance, (px - x, py - y)));
                    }
                }
            }
        }
        best.map(|(_, offset)| offset)
    };
    let mut checked = 0usize;
    let mut aligned = 0usize;
    let mut offsets = Vec::new();
    for line in layer.lines.iter().filter(|line| line.read_only.is_none()) {
        for character in line
            .chars
            .iter()
            .filter(|character| !character.synthetic && !character.character.is_whitespace())
        {
            // Text PDFium cannot map to Unicode is not comparable, nor are
            // characters its per-character API cannot return (outside the
            // Basic Multilingual Plane, as maths letters are).
            if !character.character.is_alphanumeric() || u32::from(character.character) > 0xFFFF {
                continue;
            }
            checked += 1;
            let point = baseline_point(line, character.x);
            if near(character.character, point, ALIGN_TOLERANCE).is_some() {
                aligned += 1;
            } else if let Some(offset) = near(character.character, point, 60.) {
                offsets.push(offset);
            }
        }
    }
    report.checked_chars += checked;
    report.aligned_chars += aligned;
    let misaligned = checked >= 20 && (aligned as f64) < ALIGNED_SHARE * checked as f64;
    if misaligned {
        offsets.sort_by(|a, b| a.0.total_cmp(&b.0));
        let median = offsets.get(offsets.len() / 2).copied();
        report.misaligned_pages.push(PageIssue {
            page: page_index,
            detail: format!(
                "{aligned}/{checked} characters align; median offset of the rest {}",
                median.map_or("none nearby".into(), |(dx, dy)| format!(
                    "({dx:.1}, {dy:.1}) pt"
                ))
            ),
        });
    }

    // Render as the app does and look for ink under each line.
    let Some(space) = page_space(document, page_index) else {
        return Ok(());
    };
    let view = space.view_box();
    let swapped = matches!(
        space.rotation(),
        SpaceRotation::Degrees90 | SpaceRotation::Degrees270
    );
    let (page_width, page_height) = if swapped {
        (view.height, view.width)
    } else {
        (view.width, view.height)
    };
    let (page_width, page_height) = (
        page_width * space.user_unit(),
        page_height * space.user_unit(),
    );
    let scale = (MAX_RENDER_PIXELS / (page_width * page_height))
        .sqrt()
        .min(1.5);
    let (width, height) = (
        (page_width * scale).ceil().max(1.) as u32,
        (page_height * scale).ceil().max(1.) as u32,
    );
    let request = RenderRequest {
        job_id: JobId(1),
        session_id: SessionId(1),
        page_index: page_index as u32,
        annotation_mode: AnnotationRenderMode::None,
        // As `DocumentResource` asks: PDFium draws the crop box from the
        // bitmap's corner.
        transform: [scale as f32, 0., 0., scale as f32, 0., 0.],
        clip: ClipRect {
            x: 0,
            y: 0,
            width,
            height,
        },
        surface: SurfaceDescriptor {
            surface_id: SurfaceId(1),
            width,
            height,
            stride: width * 4,
            byte_len: u64::from(width) * u64::from(height) * 4,
            format: SurfaceFormat::Bgra8Premultiplied,
        },
    };
    let mut pixels = vec![0u8; request.surface.byte_len as usize];
    backend
        .render_crop(pdfium, &request, &mut pixels, &AtomicBool::new(false))
        .map_err(|error| format!("render: {error:?}"))?;
    let transform = PageTransform::from_page_coordinate_space(space, scale / space.user_unit())
        .map_err(|error| format!("{error:?}"))?;
    let colour = |x: u32, y: u32| {
        let at = ((y * width + x) * 4) as usize;
        [pixels[at], pixels[at + 1], pixels[at + 2]]
    };
    let mut inkless = Vec::new();
    let mut boxes = Vec::new();
    for line in layer.lines.iter().filter(|line| line.read_only.is_none()) {
        let mut visible = line
            .chars
            .iter()
            .filter(|character| !character.character.is_whitespace());
        let Some(first) = visible.next() else {
            continue;
        };
        let last = visible.next_back().unwrap_or(first);
        let quad = line.quad(first.x, last.x + last.advance);
        let corners = quad.map(|(x, y)| transform.point_to_local_pixels(PdfPoint { x, y }));
        let x0 = corners.iter().map(|p| p.x).fold(f64::MAX, f64::min).max(0.) as u32;
        let x1 = (corners.iter().map(|p| p.x).fold(f64::MIN, f64::max).ceil() as u32).min(width);
        let y0 = corners.iter().map(|p| p.y).fold(f64::MAX, f64::min).max(0.) as u32;
        let y1 = (corners.iter().map(|p| p.y).fold(f64::MIN, f64::max).ceil() as u32).min(height);
        // Wholly outside the visible page: not drawn, not checked.
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        boxes.push((x0, y0, x1, y1));
        report.ink_lines += 1;
        // Ink: a pixel that differs from the box's own background (its
        // corners' most common colour), so pale text on white counts.
        let (xr, yb) = (x1 - 1, y1 - 1);
        let corners = [
            colour(x0, y0),
            colour(xr, y0),
            colour(x0, yb),
            colour(xr, yb),
        ];
        let background = *corners
            .iter()
            .max_by_key(|corner| corners.iter().filter(|other| other == corner).count())
            .expect("four corners");
        let has_ink = (y0..y1).any(|y| {
            (x0..x1).any(|x| {
                let pixel = colour(x, y);
                (0..3).any(|channel| pixel[channel].abs_diff(background[channel]) > INK_CONTRAST)
            })
        });
        if !has_ink {
            report.inkless_lines += 1;
            inkless.push(line.text.trim().chars().take(40).collect::<String>());
        }
    }
    let inkless_page = inkless.len() >= 3 && inkless.len() * 5 > boxes.len();
    if inkless_page {
        report.inkless_pages.push(PageIssue {
            page: page_index,
            detail: format!(
                "{} of {} outlined lines cover no ink, e.g. {:?}",
                inkless.len(),
                boxes.len(),
                inkless.first()
            ),
        });
    }
    if (misaligned || inkless_page) && report.crops.len() < 2 {
        let mut image = image::RgbaImage::new(width, height);
        for (index, pixel) in image.pixels_mut().enumerate() {
            let at = index * 4;
            *pixel = image::Rgba([pixels[at + 2], pixels[at + 1], pixels[at], 255]);
        }
        let mut stroke = |x: u32, y: u32, color: [u8; 4]| {
            if x < width && y < height {
                image.put_pixel(x, y, image::Rgba(color));
            }
        };
        for (x0, y0, x1, y1) in &boxes {
            for x in *x0..*x1 {
                stroke(x, *y0, [37, 99, 235, 255]);
                stroke(x, y1.saturating_sub(1), [37, 99, 235, 255]);
            }
            for y in *y0..*y1 {
                stroke(*x0, y, [37, 99, 235, 255]);
                stroke(x1.saturating_sub(1), y, [37, 99, 235, 255]);
            }
        }
        for (x, y) in origins {
            let point = transform.point_to_local_pixels(PdfPoint { x, y });
            stroke(point.x as u32, point.y as u32, [220, 38, 38, 255]);
            stroke(point.x as u32 + 1, point.y as u32, [220, 38, 38, 255]);
        }
        let name = format!("{:016x}-p{page_index}.png", {
            use std::hash::{Hash as _, Hasher as _};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            report.path.hash(&mut hasher);
            hasher.finish()
        });
        if image.save(crops_dir.join(&name)).is_ok() {
            report.crops.push(name);
        }
    }
    Ok(())
}

/// No-op and typed edits of `block` on copies of `document`: the no-op keeps
/// every content byte; the typed text reads back and no other line moves.
/// The typed edit `round_trip` makes, rebuilt for a failure's copy.
fn state_edits(layer: &Arc<PageTextLayer>, block: &PageTextBlock) -> Vec<PageTextEdit> {
    let Some(paragraph) = Paragraph::new(layer.clone(), block) else {
        return Vec::new();
    };
    let Ok(mut state) = PageTextEditState::new(paragraph, &[], &[], &[], 0) else {
        return Vec::new();
    };
    let end = state.text().chars().count();
    state.move_to(end, false);
    state.insert(TYPED);
    state.edits()
}

/// Keeps an edited copy beside the crops so a failure can be inspected.
fn keep_failure(
    document: &PdfMetadataDocument,
    edits: &[PageTextEdit],
    path: &Path,
    page: u32,
    crops_dir: &Path,
) {
    let mut copy = super::editable_copy(document);
    if apply_text_edits(&mut copy, edits, &TextEditLimits::default()).is_err() {
        return;
    }
    let dir = crops_dir.with_file_name("failures");
    let _ = std::fs::create_dir_all(&dir);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    // The worker's own preview: an incremental update of the file.
    if let Some(update) = std::fs::read(path)
        .ok()
        .and_then(|bytes| super::incremental_update(document, &bytes, &copy))
    {
        let _ = std::fs::write(dir.join(format!("{stem}-p{page}-update.pdf")), update);
    }
    let _ = copy.save(dir.join(format!("{stem}-p{page}.pdf")));
}

fn round_trip(
    document: &PdfMetadataDocument,
    layer: &Arc<PageTextLayer>,
    block: &PageTextBlock,
    report: &mut DocReport,
) -> Result<Vec<PageTextEdit>, String> {
    let page_id = *document
        .get_pages()
        .get(&(layer.page_index + 1))
        .ok_or("page is missing")?;
    let content = document.get_page_content(page_id);
    let first = layer.line(&block.lines[0]).ok_or("block line is missing")?;
    let noop = PageTextEdit {
        page_index: layer.page_index,
        anchor: first.anchor,
        original: first.original_runs(),
        runs: first.original_runs(),
        layout: Default::default(),
    };
    let mut copy = document.clone();
    apply_text_edits(
        &mut copy,
        std::slice::from_ref(&noop),
        &TextEditLimits::default(),
    )
    .map_err(|error| format!("no-op edit failed: {error}"))?;
    if copy.get_page_content(page_id) != content {
        report.noop_changed += 1;
    }

    let paragraph = Paragraph::new(layer.clone(), block).ok_or("paragraph does not open")?;
    let mut state = PageTextEditState::new(paragraph, &[], &[], &[], 0)
        .map_err(|error| format!("editor does not open: {error}"))?;
    let end = state.text().chars().count();
    state.move_to(end, false);
    if !state.insert(TYPED) {
        return Err("the paragraph's fonts cannot draw the typed text".into());
    }
    let edits = state.edits();
    let started = Instant::now();
    let mut copy = document.clone();
    apply_text_edits(&mut copy, &edits, &TextEditLimits::default())
        .map_err(|error| format!("typed edit failed: {error}"))?;
    let mut bytes = Vec::new();
    copy.save_to(&mut bytes)
        .map_err(|error| format!("edited PDF does not save: {error}"))?;
    let reopened = PdfMetadataDocument::load_mem(&bytes)
        .map_err(|error| format!("edited PDF does not reopen: {error}"))?;
    report.apply_max_ms = report.apply_max_ms.max(ms(started));
    let after = extract_page_text(&reopened, layer.page_index, &TextEditLimits::default())
        .map_err(|error| format!("edited page does not read: {error}"))?;
    let all = after
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if !all.contains(TYPED.trim()) {
        return Err("the typed text does not read back".into());
    }
    // Every other line keeps its text and place.
    let key = |line: &PageTextLine| {
        let (x, y) = line.origin();
        (
            line.text.clone(),
            (x * 10.).round() as i64,
            (y * 10.).round() as i64,
        )
    };
    let before = layer
        .lines
        .iter()
        .filter(|line| !block.lines.contains(&line.anchor) && !line.text.trim().is_empty())
        .map(key)
        .collect::<Vec<_>>();
    let after_keys = after
        .lines
        .iter()
        .map(key)
        .collect::<std::collections::HashSet<_>>();
    // A line still drawn in place may read as part of a longer one, as when
    // typed text runs up to the next table cell.
    let in_place = |(text, x, y): &(String, i64, i64)| {
        after.lines.iter().any(|line| {
            let (lx, ly) = line.origin();
            let (lx, ly) = ((lx * 10.).round() as i64, (ly * 10.).round() as i64);
            (ly - y).abs() <= 2
                && lx <= *x + 2
                && line.text.contains(text.trim())
                && line.chars.iter().any(|character| {
                    let (cx, _) = line.quad(character.x, character.x)[0];
                    ((cx * 10.).round() as i64 - x).abs() <= 2
                })
        })
    };
    if let Some(moved) = before
        .iter()
        .find(|line| !after_keys.contains(line) && !in_place(line))
    {
        return Err(format!(
            "another line changed: {:?}",
            moved.0.chars().take(40).collect::<String>()
        ));
    }
    Ok(edits)
}
