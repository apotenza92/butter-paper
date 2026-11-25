//! The search engine against real PDFium and the real PDF worker, on PDFs
//! this file builds: text across lines and paragraphs, a page with no text,
//! ligatures, markups with author, subject, text and replies, edited text,
//! and a large document that proves results arrive page by page and a
//! cancelled search stops.
//!
//! Run with
//! `BP_PDFIUM_LIBRARY=<approved-library> cargo test --test search_engine -- --ignored --nocapture`.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use butter_paper::{
    document_resource::{
        DocumentId, NativeDocumentOpener, OpenDocumentRequest, OpenedNativeDocument,
        PdfiumWorkerBackend,
    },
    pdf_text_edit::PageTextEdit,
    search_document::{DocumentTextEdits, ResourceSearchDocument},
    search_markups::markup_records,
    search_query::{SearchOptions, SearchQuery},
    search_run::{
        HitTarget, SearchConfig, SearchDocument, SearchEvent, SearchHit, SearchResults, SearchRun,
        SearchSort,
    },
    search_text_cache::SharedSearchTextCache,
};
use lopdf::{Dictionary, Document, Object, Stream, StringFormat, dictionary};

// Building PDFs

#[derive(Default)]
struct PageSpec {
    width: f64,
    height: f64,
    /// Text lines: x, y, the bytes of the string (PDF string escapes allowed).
    lines: Vec<(f64, f64, String)>,
    /// Scattered short diagonal segments, as a drawing has.
    segments: usize,
    /// Long horizontal and vertical rules (a grid) and filled rectangles.
    grid_lines: usize,
    filled_rects: usize,
    /// Content stream operators drawn after the grid and before the text.
    extra_content: String,
    /// The page's annotations, built with the ids of earlier ones.
    annotations: Vec<fn(&[lopdf::ObjectId]) -> Dictionary>,
}

fn page(lines: &[(f64, f64, &str)]) -> PageSpec {
    PageSpec {
        width: 595.,
        height: 842.,
        lines: lines
            .iter()
            .map(|(x, y, text)| (*x, *y, (*text).to_owned()))
            .collect(),
        ..PageSpec::default()
    }
}

fn pdf(pages: Vec<PageSpec>) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
        "Encoding" => "MacRomanEncoding",
    });
    let mut kids = Vec::new();
    for (index, spec) in pages.into_iter().enumerate() {
        let mut content = String::new();
        let mut seed = 99_u64 + index as u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) % 10_000) as f64 / 10_000.
        };
        content.push_str("q 0.6 G 0.4 w\n");
        for _ in 0..spec.segments {
            let (x, y) = (
                10. + (spec.width - 30.) * next(),
                10. + (spec.height - 30.) * next(),
            );
            content.push_str(&format!(
                "{x:.2} {y:.2} m {:.2} {:.2} l S\n",
                x + 6. * next(),
                y + 6. * next()
            ));
        }
        for line in 0..spec.grid_lines {
            let offset = 20. + (spec.height - 40.) * line as f64 / spec.grid_lines.max(1) as f64;
            let across = 20. + (spec.width - 40.) * line as f64 / spec.grid_lines.max(1) as f64;
            content.push_str(&format!(
                "20 {offset:.2} m {:.2} {offset:.2} l S\n{across:.2} 20 m {across:.2} {:.2} l S\n",
                spec.width - 20.,
                spec.height - 20.
            ));
        }
        content.push_str("Q\n0.9 g\n");
        for _ in 0..spec.filled_rects {
            let (x, y) = (
                10. + (spec.width - 60.) * next(),
                10. + (spec.height - 60.) * next(),
            );
            content.push_str(&format!(
                "{x:.2} {y:.2} {:.2} {:.2} re f\n",
                4. + 20. * next(),
                4. + 20. * next()
            ));
        }
        content.push_str("0 g\n");
        content.push_str(&spec.extra_content);
        for (x, y, text) in &spec.lines {
            content.push_str(&format!("BT /F1 12 Tf 1 0 0 1 {x} {y} Tm ({text}) Tj ET\n"));
        }
        let content_id = document.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = document.new_object_id();
        let mut annotation_ids = Vec::new();
        for build in &spec.annotations {
            let dictionary = build(&annotation_ids);
            annotation_ids.push(document.add_object(dictionary));
        }
        let mut page = dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), spec.width.into(), spec.height.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        };
        if !annotation_ids.is_empty() {
            page.set(
                "Annots",
                annotation_ids
                    .iter()
                    .map(|id| Object::Reference(*id))
                    .collect::<Vec<_>>(),
            );
        }
        document.objects.insert(page_id, Object::Dictionary(page));
        kids.push(Object::Reference(page_id));
    }
    let count = kids.len() as i64;
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => count,
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

fn text_string(value: &str) -> Object {
    Object::String(value.as_bytes().to_vec(), StringFormat::Literal)
}

fn rect() -> Object {
    Object::Array(vec![50.into(), 600.into(), 150.into(), 650.into()])
}

// Opening and searching

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
            .join(format!("search-engine-{name}-{}", std::process::id()));
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

fn search_document(
    id: u64,
    name: &str,
    opened: &OpenedNativeDocument,
    edits: DocumentTextEdits,
    cache: &SharedSearchTextCache,
) -> ResourceSearchDocument {
    let markups = markup_records(opened.annotations(), opened.markup_metadata(), "tester");
    ResourceSearchDocument::new(
        id,
        name,
        opened.page_count() as u32,
        opened.resource(),
        1,
        edits,
        Arc::new(markups),
        cache.clone(),
    )
}

fn search(
    documents: Vec<Arc<dyn SearchDocument>>,
    query: &str,
    options: SearchOptions,
) -> (Vec<SearchEvent>, SearchResults) {
    let names = documents
        .iter()
        .map(|d| (d.id(), d.name().to_owned(), d.page_count()))
        .collect::<Vec<_>>();
    let (sender, receiver) = mpsc::channel();
    let sender = Mutex::new(sender);
    let run = SearchRun::start(
        documents,
        Arc::new(SearchQuery::parse(query, options)),
        SearchConfig::default(),
        move |event| {
            let _ = sender.lock().unwrap().send(event);
        },
    );
    run.join();
    let events = receiver.try_iter().collect::<Vec<_>>();
    let mut results = SearchResults::new(names);
    for event in events.iter().cloned() {
        results.apply(event);
    }
    (events, results)
}

fn hits_on(results: &SearchResults, page_index: u32) -> Vec<&SearchHit> {
    results
        .hits(SearchSort::DocumentOrder)
        .into_iter()
        .filter(|hit| hit.page_index == page_index)
        .collect()
}

fn text_pages() -> Vec<PageSpec> {
    // Page 0: a paragraph whose phrase breaks across two of its lines (the
    // font is monospaced, so the first two lines are full and the app joins
    // them), then a line that ends with the first word and a line below it
    // that begins with the second.
    let mut first = page(&[
        (50., 780., "Doors to stair 2 and the lobby are a fire"),
        (50., 764., "door of 60 min rating, see the schedule."),
        (50., 748., "for details."),
        (50., 600., "door frames: steel. The fire"),
        (50., 400., "door D-101 and D-22 are listed."),
        (50., 300., "of\\336ce wing"),
    ]);
    first.annotations = vec![
        |_| {
            dictionary! {
                "Type" => "Annot", "Subtype" => "Text", "NM" => text_string("note"),
                "T" => text_string("Sam"), "Subj" => text_string("Note"),
                "Contents" => text_string("Check the wooden gate"),
                "M" => text_string("D:20261001120000"), "Rect" => rect(),
            }
        },
        |ids| {
            dictionary! {
                "Type" => "Annot", "Subtype" => "Text", "NM" => text_string("reply"),
                "T" => text_string("Kim"), "Contents" => text_string("Agreed, 30 min"),
                "IRT" => Object::Reference(ids[0]), "Rect" => rect(),
            }
        },
        |_| {
            dictionary! {
                "Type" => "Annot", "Subtype" => "Square", "NM" => text_string("frame"),
                "T" => text_string("Alex"), "Subj" => text_string("Rectangle"),
                "Contents" => text_string("Boundary of the steel frame"),
                "C" => vec![1.into(), 0.into(), 0.into()], "Rect" => rect(),
            }
        },
    ];
    // Page 1: no text, only line work.
    let drawing = PageSpec {
        width: 595.,
        height: 842.,
        segments: 50,
        ..PageSpec::default()
    };
    // Page 2: one more match, in capitals.
    let last = page(&[(50., 700., "FIRE DOOR SCHEDULE")]);
    vec![first, drawing, last]
}

#[test]
#[ignore = "requires the checksum-pinned development PDFium library"]
fn search_reads_real_pages_text_across_lines_a_page_without_text_and_markups() {
    let env = Env::new("basics");
    let opened = env.open(1, "basics.pdf", &pdf(text_pages()));
    let cache = SharedSearchTextCache::default();
    let document: Arc<dyn SearchDocument> = Arc::new(search_document(
        1,
        "basics.pdf",
        &opened,
        DocumentTextEdits::default(),
        &cache,
    ));
    let run = |query: &str, options| search(vec![document.clone()], query, options);

    // A phrase that breaks across two lines of a paragraph is found, and the
    // same two words in two paragraphs are not a phrase.
    let (_, results) = run("\"fire door\"", SearchOptions::default());
    let page0 = hits_on(&results, 0);
    let texts = page0
        .iter()
        .filter(|hit| matches!(hit.target, HitTarget::Text { .. }))
        .collect::<Vec<_>>();
    assert_eq!(texts.len(), 1, "{page0:#?}");
    assert_eq!(texts[0].snippet.matched, "fire door");
    assert_eq!(texts[0].quads.len(), 2, "one box on each of the two lines");
    // The capitals on page 2 match too (case is ignored by default).
    assert_eq!(hits_on(&results, 2).len(), 1);
    // Page 1 has no text, so nothing there and it is counted.
    assert!(hits_on(&results, 1).is_empty());
    assert_eq!(results.documents()[0].pages_without_text, 1);
    assert_eq!(results.documents()[0].pages_searched, 3);

    // Case and whole word.
    let (_, results) = run(
        "fire door",
        SearchOptions {
            match_case: true,
            whole_word: false,
        },
    );
    assert!(hits_on(&results, 2).is_empty(), "capitals do not match");
    let (_, results) = run(
        "door",
        SearchOptions {
            match_case: false,
            whole_word: true,
        },
    );
    let page0_texts = hits_on(&results, 0)
        .into_iter()
        .filter(|hit| matches!(hit.target, HitTarget::Text { .. }))
        .count();
    // "door" (twice as a word: "fire door of", "door frames", "door D-101")
    // but not inside "Doors".
    assert_eq!(page0_texts, 3, "{:#?}", hits_on(&results, 0));

    // Regular expression, OR and exclusion.
    let (_, results) = run(r"/D-\d{3}/", SearchOptions::default());
    assert_eq!(hits_on(&results, 0)[0].snippet.matched, "D-101");
    let (_, results) = run("schedule OR frames -capitals", SearchOptions::default());
    assert!(!hits_on(&results, 0).is_empty());
    let (_, results) = run("steel -frames", SearchOptions::default());
    assert!(
        hits_on(&results, 0)
            .iter()
            .all(|hit| !matches!(hit.target, HitTarget::Text { .. })),
        "the page has `frames`, so its text is out"
    );

    // Ligatures: the page draws "office" with the ligature "ﬁ" (a single
    // glyph), and a query with the letters or with the ligature finds it.
    for query in ["\"office wing\"", "\"of\u{FB01}ce wing\"", "\"fice wing\""] {
        let (_, results) = run(query, SearchOptions::default());
        let found = hits_on(&results, 0)
            .into_iter()
            .filter(|hit| matches!(hit.target, HitTarget::Text { .. }))
            .count();
        assert_eq!(found, 1, "query {query:?}");
    }

    // Markups: author, subject, text and replies.
    let (_, results) = run("author:sam", SearchOptions::default());
    let hits = hits_on(&results, 0);
    assert_eq!(hits.len(), 1, "{hits:#?}");
    assert!(matches!(
        &hits[0].target,
        HitTarget::Markup {
            in_model: false,
            ..
        }
    ));
    assert_eq!(
        hits[0].snippet.after,
        "Check the wooden gate Agreed, 30 min"
    );
    assert!(hits[0].bounds.is_some());
    let (_, results) = run("agreed", SearchOptions::default());
    let hits = hits_on(&results, 0);
    assert_eq!(hits.len(), 1);
    assert!(matches!(
        &hits[0].target,
        HitTarget::Markup { reply: Some(0), .. }
    ));
    let (_, results) = run("author:kim", SearchOptions::default());
    assert_eq!(hits_on(&results, 0).len(), 1, "a reply's author counts");
    let (_, results) = run("subject:note", SearchOptions::default());
    assert_eq!(hits_on(&results, 0).len(), 1);
    let (_, results) = run("steel", SearchOptions::default());
    // One in the page text ("door frames: steel") and one in the markup.
    assert_eq!(hits_on(&results, 0).len(), 2, "{:#?}", hits_on(&results, 0));
    let (_, results) = run("markup:steel", SearchOptions::default());
    assert_eq!(hits_on(&results, 0).len(), 1);
    let (_, results) = run("text:steel", SearchOptions::default());
    assert_eq!(hits_on(&results, 0).len(), 1);
    // Replies are not markups of their own.
    let (_, results) = run("subject:text", SearchOptions::default());
    assert_eq!(results.hit_count(), 0);

    // The pages' text is cached: a second search needs no worker.
    assert_eq!(cache.lock().len(), 3);
    drop(opened);
}

#[test]
#[ignore = "requires the checksum-pinned development PDFium library"]
fn a_phrase_runs_across_two_adjacent_lines_but_not_across_a_table_rule() {
    // Two short lines, one under the other, in the same column. They are not
    // one paragraph (a 12 pt gap is no paragraph for the page's blocks when
    // a rule lies between), so only the join of adjacent lines and the rule
    // between them decide. Page 0 has no rule; page 1 has one between them.
    let plain = page(&[(50., 700., "fire"), (50., 688., "door rating")]);
    let mut ruled = page(&[(50., 700., "fire"), (50., 688., "door rating")]);
    // A thin filled rectangle, as a table's rule is drawn, between the lines.
    ruled.extra_content = "40 695 160 0.6 re f\n".to_owned();
    let env = Env::new("line-join");
    let opened = env.open(1, "join.pdf", &pdf(vec![plain, ruled]));
    let cache = SharedSearchTextCache::default();
    let document: Arc<dyn SearchDocument> = Arc::new(search_document(
        1,
        "join.pdf",
        &opened,
        DocumentTextEdits::default(),
        &cache,
    ));
    let (_, results) = search(vec![document], "\"fire door\"", SearchOptions::default());
    assert_eq!(
        hits_on(&results, 0).len(),
        1,
        "no rule: the phrase runs on: {:#?}",
        hits_on(&results, 0)
    );
    assert_eq!(
        hits_on(&results, 1).len(),
        0,
        "a rule between the lines keeps them apart"
    );
}

#[test]
#[ignore = "requires the checksum-pinned development PDFium library"]
fn edited_text_is_found_and_the_text_it_replaced_is_not() {
    let env = Env::new("edits");
    let opened = env.open(
        1,
        "edits.pdf",
        &pdf(vec![page(&[
            (50., 700., "Fire door 60 min"),
            (50., 600., "Wooden gate"),
        ])]),
    );
    let resource = opened.resource();
    let layer = resource.page_text(0).unwrap();
    let line = layer
        .lines
        .iter()
        .find(|line| line.text.starts_with("Fire"))
        .expect("the line is read");
    let mut runs = line.original_runs();
    runs[0].text = "Steel hatch 90 min".to_owned();
    let edits = DocumentTextEdits {
        text_edits: vec![PageTextEdit {
            page_index: 0,
            anchor: line.anchor,
            original: line.original_runs(),
            runs,
            layout: Default::default(),
        }],
        ..DocumentTextEdits::default()
    };
    // One cache for every search below: a page read with other edits is
    // read again, not taken from it.
    let cache = SharedSearchTextCache::default();
    let find = |edits: DocumentTextEdits, query: &str| {
        let document: Arc<dyn SearchDocument> =
            Arc::new(search_document(1, "edits.pdf", &opened, edits, &cache));
        search(vec![document], query, SearchOptions::default())
            .1
            .hit_count()
    };
    assert_eq!(
        find(edits.clone(), "\"steel hatch\""),
        1,
        "the edit is found"
    );
    assert_eq!(
        find(edits.clone(), "\"fire door\""),
        0,
        "the text it replaced is not"
    );
    assert_eq!(
        find(edits.clone(), "\"wooden gate\""),
        1,
        "other lines stay"
    );
    assert_eq!(find(DocumentTextEdits::default(), "\"steel hatch\""), 0);
    assert_eq!(find(DocumentTextEdits::default(), "\"fire door\""), 1);
    assert_eq!(find(edits, "\"steel hatch\""), 1, "and back");
}

#[test]
#[ignore = "requires the checksum-pinned development PDFium library"]
fn text_only_reading_gives_the_same_lines_without_objects_and_measures_an_a1_drawing() {
    let env = Env::new("a1");
    // A1 is 1684 by 2384 points: a drawing with a grid of rules, thousands
    // of fills and line segments, and a title block.
    let drawing = PageSpec {
        width: 1684.,
        height: 2384.,
        lines: (0..120)
            .map(|index| {
                (
                    60. + (index % 8) as f64 * 200.,
                    80. + (index / 8) as f64 * 20.,
                    format!("Room {index} fire door 60 min"),
                )
            })
            .collect(),
        segments: 20_000,
        grid_lines: 400,
        filled_rects: 3_000,
        extra_content: String::new(),
        annotations: Vec::new(),
    };
    let opened = env.open(
        1,
        "a1.pdf",
        &pdf(vec![drawing, page(&[(50., 700., "Plain text page")])]),
    );
    let resource = opened.resource();
    let timed = |name: &str, read: &dyn Fn() -> butter_paper::pdf_text_edit::PageTextLayer| {
        let started = Instant::now();
        let layer = read();
        let elapsed = started.elapsed();
        let json = serde_json::to_vec(&layer).unwrap();
        let part = |value: &dyn erased_json::Json| value.size();
        eprintln!(
            "{name}: {:.0} ms, response {} bytes (lines {}, rules {}, fills {}, objects {})",
            elapsed.as_secs_f64() * 1000.,
            json.len(),
            part(&layer.lines),
            part(&layer.rules),
            part(&layer.fills),
            part(&layer.objects),
        );
        (layer, json.len(), elapsed)
    };
    let (mut full, full_bytes, full_time) = timed("full read", &|| resource.page_text(0).unwrap());
    let (search_layer, search_bytes, search_time) = timed("text-only read", &|| {
        resource.page_search_text(0, None, &[]).unwrap()
    });
    assert!(!full.objects.is_empty(), "the drawing has movable objects");
    assert!(search_layer.objects.is_empty());
    full.objects.clear();
    assert_eq!(search_layer, full, "all but the objects are the same");
    eprintln!(
        "text-only is {:.1}% of the full response and {:.1}% of its time",
        100. * search_bytes as f64 / full_bytes as f64,
        100. * search_time.as_secs_f64() / full_time.as_secs_f64()
    );
    assert!(search_bytes < full_bytes);
    let lines = search_layer.lines.len();
    assert_eq!(lines, 120);
    // The text is found in the drawing.
    let cache = SharedSearchTextCache::default();
    let document: Arc<dyn SearchDocument> = Arc::new(search_document(
        1,
        "a1.pdf",
        &opened,
        DocumentTextEdits::default(),
        &cache,
    ));
    let (_, results) = search(vec![document], "room 7 fire", SearchOptions::default());
    assert!(results.hit_count() >= 1);
}

/// Sizes of serialisable parts, without naming each type.
mod erased_json {
    pub trait Json {
        fn size(&self) -> usize;
    }
    impl<T: serde::Serialize> Json for T {
        fn size(&self) -> usize {
            serde_json::to_vec(self).map_or(0, |bytes| bytes.len())
        }
    }
}

fn many_pages(count: usize) -> Vec<PageSpec> {
    (0..count)
        .map(|index| {
            let mut spec = page(&[]);
            spec.segments = 300;
            spec.lines = (0..14)
                .map(|row| {
                    (
                        50.,
                        780. - row as f64 * 16.,
                        format!(
                            "Page {index} row {row}: fire door schedule and notes, item {}",
                            index * 14 + row
                        ),
                    )
                })
                .collect();
            spec
        })
        .collect()
}

#[test]
#[ignore = "requires the checksum-pinned development PDFium library"]
fn a_large_document_gives_results_page_by_page_and_a_cancelled_search_stops() {
    const PAGES: usize = 400;
    let env = Env::new("large");
    let opened = env.open(1, "large.pdf", &pdf(many_pages(PAGES)));
    let cache = SharedSearchTextCache::default();
    let document: Arc<dyn SearchDocument> = Arc::new(search_document(
        1,
        "large.pdf",
        &opened,
        DocumentTextEdits::default(),
        &cache,
    ));
    let query = Arc::new(SearchQuery::parse(
        "\"item 3000\"",
        SearchOptions::default(),
    ));

    // A whole search, to see how long it takes and that results come in
    // before it ends.
    let (sender, receiver) = mpsc::channel();
    let sender = Mutex::new(sender);
    let started = Instant::now();
    let run = SearchRun::start(
        vec![document.clone()],
        query.clone(),
        SearchConfig::default(),
        move |event| {
            let _ = sender.lock().unwrap().send((Instant::now(), event));
        },
    );
    run.join();
    let total = started.elapsed();
    let events = receiver.try_iter().collect::<Vec<_>>();
    let pages = events
        .iter()
        .filter(|(_, event)| matches!(event, SearchEvent::Page(_)))
        .collect::<Vec<_>>();
    assert_eq!(pages.len(), PAGES);
    let first_at = pages[0].0 - started;
    let hit_page = events
        .iter()
        .find_map(|(at, event)| match event {
            SearchEvent::Page(page) if !page.hits.is_empty() => {
                Some((*at - started, page.page_index))
            }
            _ => None,
        })
        .expect("a page has the item");
    assert_eq!(hit_page.1, 3000 / 14);
    assert!(
        first_at * 20 < total,
        "the first page ({first_at:?}) arrives long before the end ({total:?})"
    );
    eprintln!(
        "cold search of {PAGES} pages: {:.0} ms total, first page after {:.0} ms, {:.1} ms a page",
        total.as_secs_f64() * 1000.,
        first_at.as_secs_f64() * 1000.,
        total.as_secs_f64() * 1000. / PAGES as f64
    );
    assert_eq!(cache.lock().len(), PAGES);

    // The same search again takes its text from the cache.
    let started = Instant::now();
    let (_, results) = search(
        vec![document.clone()],
        "\"item 3000\"",
        SearchOptions::default(),
    );
    let warm = started.elapsed();
    assert_eq!(results.hit_count(), 1);
    eprintln!("warm search: {:.0} ms", warm.as_secs_f64() * 1000.);
    assert!(warm < total / 3, "{warm:?} against {total:?}");

    // Cancelling: a cold search of a document that is not cached stops soon
    // after the cancel, with most pages unread.
    let cold = Arc::new(search_document(
        2,
        "large.pdf",
        &opened,
        DocumentTextEdits::default(),
        &SharedSearchTextCache::default(),
    ));
    let (sender, receiver) = mpsc::channel();
    let sender = Mutex::new(sender);
    let run = SearchRun::start(vec![cold], query, SearchConfig::default(), move |event| {
        let _ = sender.lock().unwrap().send((Instant::now(), event));
    });
    for _ in 0..5 {
        receiver.recv_timeout(Duration::from_secs(30)).unwrap();
    }
    let cancelled_at = Instant::now();
    run.cancel();
    run.join();
    let stopped_after = cancelled_at.elapsed();
    let events = receiver.try_iter().collect::<Vec<_>>();
    let read = 5 + events
        .iter()
        .filter(|(_, event)| matches!(event, SearchEvent::Page(_)))
        .count();
    eprintln!("cancel: stopped {stopped_after:?} later, {read} of {PAGES} pages read");
    assert!(read < PAGES / 2, "{read} pages read");
    assert!(matches!(
        events.last(),
        Some((_, SearchEvent::Finished { cancelled: true }))
    ));
    assert!(stopped_after < Duration::from_secs(5));
}

#[test]
#[ignore = "requires the checksum-pinned development PDFium library"]
fn a_search_does_not_make_a_page_the_viewer_asks_for_wait() {
    const PAGES: usize = 300;
    let env = Env::new("priority");
    let opened = env.open(1, "priority.pdf", &pdf(many_pages(PAGES)));
    let resource = opened.resource();
    // How long one page takes with the worker idle.
    let idle = {
        let started = Instant::now();
        for page in 0..10 {
            resource.render_page(page, 600).unwrap();
        }
        started.elapsed() / 10
    };
    let document: Arc<dyn SearchDocument> = Arc::new(search_document(
        1,
        "priority.pdf",
        &opened,
        DocumentTextEdits::default(),
        &SharedSearchTextCache::default(),
    ));
    let (sender, receiver) = mpsc::channel();
    let sender = Mutex::new(sender);
    let run = SearchRun::start(
        vec![document],
        Arc::new(SearchQuery::parse("notes", SearchOptions::default())),
        SearchConfig::default(),
        move |event| {
            let _ = sender.lock().unwrap().send(event);
        },
    );
    receiver.recv_timeout(Duration::from_secs(30)).unwrap();
    // While the search runs, the viewer asks for pages.
    let mut slowest = Duration::ZERO;
    for page in 20..30 {
        let started = Instant::now();
        resource.render_page(page, 600).unwrap();
        slowest = slowest.max(started.elapsed());
    }
    let searching = receiver.try_iter().count();
    run.cancel();
    run.join();
    eprintln!(
        "render of one page: {idle:?} idle, slowest {slowest:?} during the search ({searching} pages read meanwhile)"
    );
    // At most the page being read (and a little more) is in the way.
    assert!(
        slowest < idle * 4 + Duration::from_millis(400),
        "{slowest:?} while searching against {idle:?} idle"
    );
}

#[test]
fn the_fixtures_are_valid_pdfs() {
    // Runs without PDFium: the generated files parse.
    let document = Document::load_mem(&pdf(text_pages())).unwrap();
    assert_eq!(document.get_pages().len(), 3);
}
