//! Visual search against real PDFium and the real PDF worker, on PDFs this
//! file builds: a vector door symbol repeated turned, resized and mirrored,
//! on a busy drawing, also on a page with /Rotate 90. The symbol is found
//! through the worker's search render (page content only, real colours,
//! lowest priority) and the hits land on the symbols in PDF space.
//!
//! Run with
//! `BP_PDFIUM_LIBRARY=<approved-library> cargo test --test visual_search_engine -- --ignored --nocapture`.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use butter_paper::{
    annotation_model::PageRotation,
    document_resource::{
        DocumentId, NativeDocumentOpener, OpenDocumentRequest, OpenedNativeDocument,
        PdfiumWorkerBackend,
    },
    page_geometry::PdfRect,
    visual_search::{DEFAULT_SIMILARITY, VisualSearchOptions, VisualTemplate, working_scale},
    visual_search_run::{
        ResourceVisualDocument, SharedVisualRenderCache, TemplateSource, VisualPageState,
        VisualResults, VisualSearchDocument, VisualSearchEvent, VisualSearchRun, matcher_threads,
    },
};
use lopdf::{Document, Object, Stream, dictionary};

/// A door in a 24 pt box, PDF y up: the leaf, its swing and a jamb tick.
const DOOR: &str = "1.6 w 0 0 0 RG 2 2 m 2 22 l S \
                    2 22 m 13.05 22 22 13.05 22 2 c S \
                    22 0.5 m 22 5 l S 0 0 0 rg 0.5 0.5 3 3 re f";

/// One copy: the centre of its box in PDF space, clockwise quarter turns
/// on the page (after the mirror), its size, and a left-to-right mirror.
#[derive(Clone, Copy, Debug)]
struct Copy {
    x: f64,
    y: f64,
    turn: u8,
    size: f64,
    mirrored: bool,
}

impl Copy {
    /// The `cm` that draws the 24 pt symbol box centred at (x, y).
    fn matrix(self) -> [f64; 6] {
        let angle = f64::from(self.turn) * std::f64::consts::FRAC_PI_2;
        let (sin, cos) = angle.sin_cos();
        let flip = if self.mirrored { -1. } else { 1. };
        // Clockwise on the page with y up: (x, y) -> (x cos + y sin, -x sin + y cos).
        let (l00, l01) = (cos * self.size * flip, sin * self.size);
        let (l10, l11) = (-sin * self.size * flip, cos * self.size);
        [
            l00,
            l10,
            l01,
            l11,
            self.x - (l00 * 12. + l01 * 12.),
            self.y - (l10 * 12. + l11 * 12.),
        ]
    }
}

struct PageSpec {
    width: f64,
    height: f64,
    rotate: i64,
    copies: Vec<Copy>,
    /// Short strokes and rules, as a drawing has.
    clutter: usize,
}

fn pdf(pages: &[PageSpec]) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let mut kids = Vec::new();
    for (index, spec) in pages.iter().enumerate() {
        let mut seed = 7_u64 + index as u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) % 10_000) as f64 / 10_000.
        };
        let mut content = String::from("q 0.5 G 0.5 w\n");
        for _ in 0..spec.clutter {
            let (x, y) = (spec.width * next(), spec.height * next());
            if next() < 0.5 {
                content.push_str(&format!(
                    "{x:.2} {y:.2} m {:.2} {:.2} l S\n",
                    x + 4. * next(),
                    y + 4. * next()
                ));
            } else {
                let length = 60. + 200. * next();
                content.push_str(&format!("{x:.2} {y:.2} m {:.2} {y:.2} l S\n", x + length));
            }
        }
        content.push_str("Q\n");
        for copy in &spec.copies {
            // A white ground round each copy, so clutter does not touch it.
            let side = 24. * copy.size + 20.;
            content.push_str(&format!(
                "q 1 g {:.2} {:.2} {side:.2} {side:.2} re f Q\n",
                copy.x - side / 2.,
                copy.y - side / 2.
            ));
            let [a, b, c, d, e, f] = copy.matrix();
            content.push_str(&format!(
                "q {a:.5} {b:.5} {c:.5} {d:.5} {e:.3} {f:.3} cm {DOOR} Q\n"
            ));
        }
        let content_id = document.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(spec.width as f32), Object::Real(spec.height as f32)],
            "Rotate" => spec.rotate,
            "Contents" => content_id,
            "Resources" => dictionary! {},
        });
        kids.push(page_id.into());
    }
    let count = kids.len() as i64;
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

struct Env {
    backend: PdfiumWorkerBackend,
    scratch: PathBuf,
}

impl Env {
    fn new(name: &str) -> Self {
        let library = PathBuf::from(
            std::env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY names libpdfium"),
        );
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-scratch")
            .join(format!("visual-search-{name}-{}", std::process::id()));
        std::fs::create_dir_all(scratch.join("surfaces")).unwrap();
        let worker = PathBuf::from(env!("CARGO_BIN_EXE_butter-paper-pdf-worker"));
        Self {
            backend: PdfiumWorkerBackend::new(worker, library, scratch.join("surfaces")),
            scratch,
        }
    }

    fn open(&self, id: u64, file_name: &str, bytes: &[u8]) -> OpenedNativeDocument {
        let path = self.scratch.join(file_name);
        std::fs::write(&path, bytes).unwrap();
        self.backend
            .open(&OpenDocumentRequest {
                document_id: DocumentId::new(id),
                generation: 1,
                path,
            })
            .expect("the PDF opens")
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// The pages as the app gives them to a search: as shown, with their
/// coordinate spaces.
fn page_states(opened: &OpenedNativeDocument) -> Vec<VisualPageState> {
    (0..opened.page_count() as u32)
        .map(|page| {
            let space = opened.page_coordinate_space(page);
            VisualPageState {
                rotation: space
                    .map(|space| {
                        PageRotation::from_degrees(i64::from(space.rotation().degrees())).unwrap()
                    })
                    .unwrap_or(PageRotation::Degrees0),
                space,
                validity: u64::from(page),
            }
        })
        .collect()
}

struct Search {
    results: VisualResults,
    first_hit: Option<Duration>,
    total: Duration,
    events: Vec<VisualSearchEvent>,
}

/// Draws a box round `source` on page `page`, cuts the template as the app
/// does, and searches every page.
fn search(
    opened: &OpenedNativeDocument,
    page: u32,
    source: Copy,
    options: &VisualSearchOptions,
) -> Search {
    let states = page_states(opened);
    let space = states[page as usize].space.unwrap();
    let cache = SharedVisualRenderCache::default();
    let document = Arc::new(ResourceVisualDocument::new(
        1,
        opened.resource(),
        states.clone(),
        cache,
    ));
    let side = 24. * source.size + 10.;
    let box_pdf = PdfRect {
        x: source.x - side / 2.,
        y: source.y - side / 2.,
        width: side,
        height: side,
    };
    let viewport_box = space.pdf_rect_to_viewport(box_pdf);
    let largest = states
        .iter()
        .filter_map(|state| state.space)
        .map(|space| {
            let (w, h) = space.display_size_points();
            (w as f32, h as f32)
        })
        .fold(
            (0f32, 0f32),
            |a, b| if b.0 * b.1 > a.0 * a.1 { b } else { a },
        );
    let started = Instant::now();
    let rough = working_scale(viewport_box.width.min(viewport_box.height) as f32, largest);
    let render = document.render_page(page, rough).unwrap();
    // The search render is in real colours: white paper, dark ink (Dark
    // pages would give a dark ground).
    let pixels = render.image.pixels();
    let mean = pixels.iter().map(|&v| u64::from(v)).sum::<u64>() / pixels.len() as u64;
    assert!(mean > 150, "real paper: mean grey {mean}");
    let template = VisualTemplate::from_page_box(&render.image, rough, viewport_box).unwrap();
    let trimmed = template.source_box().unwrap();
    let scale = working_scale(trimmed.width.min(trimmed.height) as f32, largest);
    let template = if scale == rough {
        template
    } else {
        let render = document.render_page(page, scale).unwrap();
        VisualTemplate::from_page_box(&render.image, scale, trimmed).unwrap()
    };
    let prepared = Arc::new(template.prepare(scale, options).unwrap());
    let events = Arc::new(Mutex::new(Vec::new()));
    let first_hit = Arc::new(Mutex::new(None));
    let sink = (events.clone(), first_hit.clone());
    let run = VisualSearchRun::start(
        vec![document.clone() as Arc<dyn VisualSearchDocument>],
        prepared,
        Some(TemplateSource {
            document_id: 1,
            page_index: page,
        }),
        matcher_threads(),
        move |event| {
            if let VisualSearchEvent::Page(outcome) = &event
                && !outcome.hits.is_empty()
            {
                sink.1.lock().unwrap().get_or_insert(started.elapsed());
            }
            sink.0.lock().unwrap().push(event);
        },
    );
    run.join();
    let total = started.elapsed();
    let events = events.lock().unwrap().clone();
    let mut results = VisualResults::new([(1, "set.pdf".to_owned(), states.len() as u32)]);
    for event in events.iter().cloned() {
        results.apply(event);
    }
    Search {
        results,
        first_hit: *first_hit.lock().unwrap(),
        total,
        events,
    }
}

fn centre(rect: PdfRect) -> (f64, f64) {
    (rect.x + rect.width / 2., rect.y + rect.height / 2.)
}

#[test]
#[ignore = "requires the checksum-pinned development PDFium library"]
fn a_vector_symbol_is_found_turned_resized_and_mirrored_through_the_worker() {
    let env = Env::new("symbols");
    let copy = |x, y, turn, size, mirrored| Copy {
        x,
        y,
        turn,
        size,
        mirrored,
    };
    let first = vec![
        copy(100., 700., 0, 1.0, false),
        copy(250., 700., 1, 1.0, false),
        copy(400., 700., 2, 1.0, true),
        copy(100., 500., 3, 1.1, false),
        copy(250., 500., 0, 0.9, true),
        copy(400., 500., 1, 1.0, true),
    ];
    let rotated = vec![
        copy(200., 300., 0, 1.0, false),
        copy(400., 300., 1, 1.0, true),
    ];
    let bytes = pdf(&[
        PageSpec {
            width: 595.,
            height: 842.,
            rotate: 0,
            copies: first.clone(),
            clutter: 600,
        },
        PageSpec {
            width: 595.,
            height: 842.,
            rotate: 90,
            copies: rotated.clone(),
            clutter: 600,
        },
    ]);
    let opened = env.open(1, "symbols.pdf", &bytes);
    let found = search(&opened, 0, first[0], &VisualSearchOptions::default());
    assert_eq!(
        found.events.last(),
        Some(&VisualSearchEvent::Finished { cancelled: false })
    );
    let hits = |page: u32| {
        found
            .results
            .page_hits(1, page)
            .iter()
            .filter(|hit| hit.hit.score >= DEFAULT_SIMILARITY)
            .collect::<Vec<_>>()
    };
    let mut worst = 0f64;
    for (page, copies, page_turn) in [(0u32, &first, 0u8), (1, &rotated, 1)] {
        let page_hits = hits(page);
        assert_eq!(page_hits.len(), copies.len(), "page {page}: {page_hits:#?}");
        for copy in copies.iter() {
            let near = page_hits
                .iter()
                .filter(|hit| {
                    let (x, y) = centre(hit.hit.pdf_rect.unwrap());
                    (x - copy.x).abs() < 6. && (y - copy.y).abs() < 6.
                })
                .collect::<Vec<_>>();
            assert_eq!(near.len(), 1, "page {page} {copy:?}: {page_hits:#?}");
            let hit = &near[0].hit;
            let (x, y) = centre(hit.pdf_rect.unwrap());
            worst = worst.max((x - copy.x).abs()).max((y - copy.y).abs());
            // On a page shown turned, the template (from page 1) is found
            // turned by the page's turn as well.
            let expected = (copy.turn + page_turn) % 4;
            assert_eq!(
                hit.rotation.degrees(),
                u16::from(expected) * 90,
                "page {page} {copy:?}: {hit:?}"
            );
            assert_eq!(hit.mirrored, copy.mirrored, "page {page} {copy:?}: {hit:?}");
            assert!(
                (f64::from(hit.size) - copy.size).abs() <= 0.06,
                "page {page} {copy:?}: {hit:?}"
            );
            eprintln!(
                "page {page} {copy:?}: score {:.3}, {}°{}, size {:.2}, off ({:+.2}, {:+.2}) pt",
                hit.score,
                hit.rotation.degrees(),
                if hit.mirrored { " mirrored" } else { "" },
                hit.size,
                x - copy.x,
                y - copy.y
            );
        }
    }
    assert!(
        worst <= 1.5,
        "hits within 1.5 pt of the symbols: {worst:.2}"
    );
    assert_eq!(
        found
            .results
            .shown_keys(DEFAULT_SIMILARITY)
            .iter()
            .filter(|key| found.results.hit(**key).unwrap().hit.source)
            .count(),
        1
    );
    eprintln!(
        "first hit after {:?}, all after {:?}; worst offset {worst:.2} pt",
        found.first_hit, found.total
    );
}

#[test]
#[ignore = "requires the checksum-pinned development PDFium library"]
fn measure_a_20_page_a1_set() {
    let env = Env::new("a1-set");
    let mut seed = 31_u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) % 10_000) as f64 / 10_000.
    };
    let mut pages = Vec::new();
    let mut expected = 0;
    for _ in 0..20 {
        let mut copies = Vec::new();
        for row in 0..3 {
            for column in 0..4 {
                copies.push(Copy {
                    x: 300. + column as f64 * 500. + 100. * next(),
                    y: 300. + row as f64 * 450. + 100. * next(),
                    turn: (next() * 4.) as u8 % 4,
                    size: [0.9, 1.0, 1.0, 1.1][(next() * 4.) as usize % 4],
                    mirrored: next() < 0.3,
                });
            }
        }
        expected += copies.len();
        pages.push(PageSpec {
            width: 2384.,
            height: 1684.,
            rotate: 0,
            copies,
            clutter: 12_000,
        });
    }
    let source = pages[0].copies[0];
    let built = Instant::now();
    let bytes = pdf(&pages);
    let opened = env.open(1, "a1-set.pdf", &bytes);
    eprintln!(
        "built and opened {} bytes in {:?}",
        bytes.len(),
        built.elapsed()
    );
    let found = search(&opened, 0, source, &VisualSearchOptions::default());
    let shown = found.results.hit_count(DEFAULT_SIMILARITY);
    eprintln!(
        "20 A1 pages, {} matcher threads: first hit after {:?}, all after {:?}; \
         {shown} hits at 80 % of {expected} placed",
        matcher_threads(),
        found.first_hit,
        found.total
    );
    assert!(shown >= expected * 9 / 10, "{shown} of {expected}");
    assert_eq!(found.results.progress(), (20, 20));
}

/// A red valve of radius `r` centred at (x, y), PDF y up: a circle, and a
/// closed bow tie that gives two vertical bars and a cross. The bow tie
/// corners are cut to whole points, as the macOS release check drew them.
fn valve(x: f64, y: f64, r: f64) -> String {
    let k = 0.5523 * r;
    let corner = |dx: f64, dy: f64| format!("{} {}", (x + dx).trunc(), (y + dy).trunc());
    let a = 0.7 * r;
    format!(
        "q 1 0 0 RG 2 w {} {y} m \
         {:.1} {:.1} {:.1} {:.1} {x} {} c {:.1} {:.1} {:.1} {:.1} {} {y} c \
         {:.1} {:.1} {:.1} {:.1} {x} {} c {:.1} {:.1} {:.1} {:.1} {} {y} c S \
         {} m {} l {} l {} l h S Q\n",
        x + r,
        x + r,
        y + k,
        x + k,
        y + r,
        y + r,
        x - k,
        y + r,
        x - r,
        y + k,
        x - r,
        x - r,
        y - k,
        x - k,
        y - r,
        y - r,
        x + k,
        y - r,
        x + r,
        y - k,
        x + r,
        corner(-a, a),
        corner(a, -a),
        corner(a, a),
        corner(-a, -a),
    )
}

/// Helvetica text with its baseline at (x, y).
fn text(x: f64, y: f64, size: f64, words: &str) -> String {
    format!("BT /F1 {size} Tf 0 g {x} {y} Td ({words}) Tj ET\n")
}

fn letter_pdf(pages: &[String]) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let mut kids = Vec::new();
    for content in pages {
        let content_id =
            document.add_object(Stream::new(dictionary! {}, content.clone().into_bytes()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        kids.push(page_id.into());
    }
    let count = kids.len() as i64;
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

/// The macOS release check drew its source valve with radius 22 pt and the
/// other valves with 18 pt (82 %, under the 85 % size floor). Each copy
/// matches at about 90 % at full resolution, but at one sub-point position
/// the 6 px coarse template scored 0.48 and the copy above a caption was
/// lost. Every copy must be found, at each phase of the coarse grid.
#[test]
#[ignore = "requires the checksum-pinned development PDFium library"]
fn a_valve_a_little_under_the_size_range_is_found_at_every_phase() {
    let env = Env::new("valves");
    let mut first = text(
        72.,
        545.,
        11.,
        "Colour bars: red orange yellow green blue purple white black",
    );
    for (index, colour) in [
        "1 0 0",
        "1 0.63 0",
        "1 1 0",
        "0 0.67 0",
        "0 0.47 1",
        "0.47 0 0.78",
        "1 1 1",
        "0 0 0",
    ]
    .iter()
    .enumerate()
    {
        first.push_str(&format!(
            "{colour} rg {} 420 20 120 re f\n",
            72 + 20 * index
        ));
    }
    let mut copies = vec![(380., 520.), (470., 470.), (380., 420.)];
    for &(x, y) in &copies {
        first.push_str(&valve(x, y, 18.));
    }
    first.push_str(&text(330., 380., 11., "Three valve symbols on this page"));
    let source = valve(150., 420., 22.) + &valve(300., 330., 18.);
    copies.push((300., 330.));
    // One copy at each whole-point offset over 8 pt, a full period of the
    // coarse grid at 0.5 px/pt.
    let mut grid = String::new();
    let mut grid_copies = Vec::new();
    for index in 0..64 {
        let (column, row) = (index % 8, index / 8);
        let (x, y) = (
            60. + 70. * f64::from(column) + f64::from(column),
            740. - 70. * f64::from(row) + f64::from(row),
        );
        grid.push_str(&valve(x, y, 18.));
        grid_copies.push((x, y));
    }
    let bytes = letter_pdf(&[first, source, grid]);
    let opened = env.open(1, "valves.pdf", &bytes);
    // A 56 pt box round the large valve, as the release check drew it.
    let box_copy = Copy {
        x: 150.,
        y: 420.,
        turn: 0,
        size: 46. / 24.,
        mirrored: false,
    };
    let found = search(&opened, 1, box_copy, &VisualSearchOptions::default());
    let mut report = Vec::new();
    for (page, expected) in [
        (0u32, &copies[..3]),
        (1, &copies[3..]),
        (2, &grid_copies[..]),
    ] {
        let shown: Vec<_> = found
            .results
            .page_hits(1, page)
            .iter()
            .filter(|hit| hit.hit.score >= DEFAULT_SIMILARITY && !hit.hit.source)
            .collect();
        for &(x, y) in expected {
            let near: Vec<_> = shown
                .iter()
                .filter(|hit| {
                    let (hx, hy) = centre(hit.hit.pdf_rect.unwrap());
                    (hx - x).abs() < 3. && (hy - y).abs() < 3.
                })
                .collect();
            if near.len() != 1 {
                report.push(format!("page {page} ({x}, {y}): {} hits", near.len()));
            }
        }
        // Text, colour bars and the large valve give no other hits.
        if shown.len() != expected.len() {
            report.push(format!(
                "page {page}: {} hits for {} valves",
                shown.len(),
                expected.len()
            ));
        }
    }
    assert!(report.is_empty(), "{report:#?}");
}
