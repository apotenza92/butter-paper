//! Visual search on synthetic line drawings that this file draws: symbols
//! (a door, a circle with a cross, an arrow) placed turned and resized on a
//! busy background of walls, grid lines, hatching, text-like strokes and
//! noise. The tests need no PDF and no worker.
//!
//! The benchmark is ignored. Run it in an optimised build with
//! `cargo test --config 'profile.dev.package.butter-paper.opt-level=3'
//! --test visual_search -- --ignored --nocapture`.

use std::{
    f32::consts::{FRAC_PI_2, PI, TAU},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use butter_paper::{
    page_geometry::{PageCoordinateSpace, PdfRect, Rotation},
    visual_search::{
        DEFAULT_SIMILARITY, GreyImage, PreparedPage, PreparedTemplate, VisualHit, VisualPage,
        VisualSearchError, VisualSearchOptions, VisualTemplate,
    },
};

/// The symbol size in pixels at scale 1, as the design's working scale aims for.
const SYMBOL_PX: f32 = 32.0;
const SYMBOL_PEN: f32 = 1.5;

/// A small deterministic generator (xorshift64*).
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }
}

#[derive(Clone, Copy)]
enum Stroke {
    Line((f32, f32), (f32, f32)),
    /// Centre, radius, start and end angle (radians, y down, clockwise).
    Arc((f32, f32), f32, f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Symbol {
    Door,
    CircleCross,
    /// The near miss of `CircleCross`: the same circle with diagonal lines.
    CircleSaltire,
    Arrow,
}

impl Symbol {
    /// Strokes in a unit box, y down.
    fn strokes(self) -> Vec<Stroke> {
        match self {
            Self::Door => vec![
                Stroke::Line((0.1, 0.9), (0.1, 0.1)),
                Stroke::Arc((0.1, 0.9), 0.8, -FRAC_PI_2, 0.0),
                Stroke::Line((0.9, 0.82), (0.9, 0.98)),
            ],
            Self::CircleCross => vec![
                Stroke::Arc((0.5, 0.5), 0.4, 0.0, TAU),
                Stroke::Line((0.5, 0.05), (0.5, 0.95)),
                Stroke::Line((0.05, 0.5), (0.95, 0.5)),
            ],
            Self::CircleSaltire => vec![
                Stroke::Arc((0.5, 0.5), 0.4, 0.0, TAU),
                Stroke::Line((0.18, 0.18), (0.82, 0.82)),
                Stroke::Line((0.18, 0.82), (0.82, 0.18)),
            ],
            Self::Arrow => vec![
                Stroke::Line((0.1, 0.5), (0.9, 0.5)),
                Stroke::Line((0.9, 0.5), (0.65, 0.28)),
                Stroke::Line((0.9, 0.5), (0.65, 0.72)),
                Stroke::Line((0.1, 0.36), (0.1, 0.64)),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Placement {
    symbol: Symbol,
    /// Top-left of the symbol's square, in pixels.
    x: f32,
    y: f32,
    /// Clockwise quarter turns.
    turn: u8,
    size: f32,
    /// Flipped left to right before the turn.
    mirrored: bool,
}

impl Placement {
    fn side(self) -> f32 {
        SYMBOL_PX * self.size
    }

    fn centre(self) -> (f32, f32) {
        (self.x + self.side() / 2.0, self.y + self.side() / 2.0)
    }

    /// Maps a unit-box point: mirror, turn about the centre, scale, move.
    fn map(self, (u, v): (f32, f32)) -> (f32, f32) {
        let u = if self.mirrored { 1.0 - u } else { u };
        let (mut du, mut dv) = (u - 0.5, v - 0.5);
        for _ in 0..self.turn {
            (du, dv) = (-dv, du);
        }
        (
            self.x + (du + 0.5) * self.side(),
            self.y + (dv + 0.5) * self.side(),
        )
    }

    fn rotation(self) -> Rotation {
        Rotation::from_degrees(i64::from(self.turn) * 90).unwrap()
    }
}

struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![255; width * height],
        }
    }

    /// Paints anti-aliased ink inside a bounding box from a distance function.
    fn paint(
        &mut self,
        bounds: (f32, f32, f32, f32),
        pen: f32,
        ink: f32,
        distance: impl Fn(f32, f32) -> f32,
    ) {
        let reach = pen / 2.0 + 1.0;
        let x0 = (bounds.0 - reach).floor().max(0.0) as usize;
        let y0 = (bounds.1 - reach).floor().max(0.0) as usize;
        let x1 = ((bounds.2 + reach).ceil().max(0.0) as usize).min(self.width);
        let y1 = ((bounds.3 + reach).ceil().max(0.0) as usize).min(self.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = distance(x as f32 + 0.5, y as f32 + 0.5);
                let cover = (pen / 2.0 + 0.5 - d).clamp(0.0, 1.0);
                if cover > 0.0 {
                    let value = 255.0 - cover * ink * 255.0;
                    let pixel = &mut self.pixels[y * self.width + x];
                    *pixel = (*pixel).min(value.round() as u8);
                }
            }
        }
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), pen: f32, ink: f32) {
        let bounds = (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1));
        self.paint(bounds, pen, ink, |px, py| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let length2 = (dx * dx + dy * dy).max(1e-6);
            let t = (((px - a.0) * dx + (py - a.1) * dy) / length2).clamp(0.0, 1.0);
            ((px - a.0 - t * dx).powi(2) + (py - a.1 - t * dy).powi(2)).sqrt()
        });
    }

    fn arc(&mut self, centre: (f32, f32), radius: f32, start: f32, end: f32, pen: f32, ink: f32) {
        let bounds = (
            centre.0 - radius,
            centre.1 - radius,
            centre.0 + radius,
            centre.1 + radius,
        );
        let point = |angle: f32| {
            (
                centre.0 + radius * angle.cos(),
                centre.1 + radius * angle.sin(),
            )
        };
        let (first, last) = (point(start), point(end));
        self.paint(bounds, pen, ink, |px, py| {
            let (dx, dy) = (px - centre.0, py - centre.1);
            let mut angle = dy.atan2(dx);
            while angle < start {
                angle += TAU;
            }
            if angle <= end {
                ((dx * dx + dy * dy).sqrt() - radius).abs()
            } else {
                let to = |p: (f32, f32)| ((px - p.0).powi(2) + (py - p.1).powi(2)).sqrt();
                to(first).min(to(last))
            }
        });
    }

    fn clear(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let x0 = x.floor().max(0.0) as usize;
        let y0 = y.floor().max(0.0) as usize;
        let x1 = ((x + width).ceil() as usize).min(self.width);
        let y1 = ((y + height).ceil() as usize).min(self.height);
        for row in y0..y1 {
            self.pixels[row * self.width + x0..row * self.width + x1].fill(255);
        }
    }

    fn place(&mut self, placement: Placement) {
        for stroke in placement.symbol.strokes() {
            match stroke {
                Stroke::Line(a, b) => {
                    self.line(placement.map(a), placement.map(b), SYMBOL_PEN, 1.0);
                }
                Stroke::Arc(centre, radius, start, end) => {
                    // A mirror takes angle a to π - a and reverses the sweep.
                    let (start, end) = if placement.mirrored {
                        (PI - end, PI - start)
                    } else {
                        (start, end)
                    };
                    let turn = f32::from(placement.turn) * FRAC_PI_2;
                    self.arc(
                        placement.map(centre),
                        radius * placement.side(),
                        start + turn,
                        end + turn,
                        SYMBOL_PEN,
                        1.0,
                    );
                }
            }
        }
    }

    fn image(self) -> GreyImage {
        GreyImage::new(self.width, self.height, self.pixels).unwrap()
    }
}

/// Draws a busy plan: grid lines, double walls, hatching, text-like strokes
/// and speckles, then clears a white area for each symbol, draws the
/// symbols and adds grey noise over everything.
fn draw_page(width: usize, height: usize, placements: &[Placement], seed: u64) -> GreyImage {
    let mut random = Random(seed);
    let mut canvas = Canvas::new(width, height);
    let (w, h) = (width as f32, height as f32);
    // Grid lines, light and thin.
    let mut x = random.range(40.0, 90.0);
    while x < w {
        canvas.line((x, 0.0), (x, h), 1.0, 0.45);
        x += random.range(140.0, 220.0);
    }
    let mut y = random.range(40.0, 90.0);
    while y < h {
        canvas.line((0.0, y), (w, y), 1.0, 0.45);
        y += random.range(140.0, 220.0);
    }
    let area = w * h / 1_000_000.0;
    // Double walls.
    for _ in 0..(40.0 * area) as usize {
        let (x, y) = (random.range(0.0, w), random.range(0.0, h));
        let length = random.range(60.0, 400.0);
        if random.unit() < 0.5 {
            canvas.line((x, y), (x + length, y), 2.0, 1.0);
            canvas.line((x, y + 7.0), (x + length, y + 7.0), 2.0, 1.0);
        } else {
            canvas.line((x, y), (x, y + length), 2.0, 1.0);
            canvas.line((x + 7.0, y), (x + 7.0, y + length), 2.0, 1.0);
        }
    }
    // Hatched areas.
    for _ in 0..(8.0 * area) as usize {
        let (x, y) = (random.range(0.0, w), random.range(0.0, h));
        let size = random.range(40.0, 120.0);
        let mut offset = 0.0;
        while offset < size * 2.0 {
            let a = (x + offset.min(size), y + (offset - size).max(0.0));
            let b = (x + (offset - size).max(0.0), y + offset.min(size));
            canvas.line(a, b, 0.8, 0.7);
            offset += 6.0;
        }
    }
    // Text-like words: short strokes 7 px high.
    for _ in 0..(120.0 * area) as usize {
        let (mut x, y) = (random.range(0.0, w), random.range(0.0, h));
        for _ in 0..random.range(3.0, 12.0) as usize {
            let kind = random.unit();
            if kind < 0.4 {
                canvas.line((x, y), (x, y + 7.0), 1.0, 1.0);
            } else if kind < 0.7 {
                canvas.arc((x + 2.0, y + 4.5), 2.5, 0.0, TAU, 1.0, 1.0);
                x += 3.0;
            } else {
                canvas.line((x, y + 7.0), (x + 3.0, y), 1.0, 1.0);
                x += 2.0;
            }
            x += random.range(2.5, 4.5);
        }
    }
    // Dimension lines with ticks.
    for _ in 0..(10.0 * area) as usize {
        let (x, y) = (random.range(0.0, w), random.range(0.0, h));
        let length = random.range(80.0, 300.0);
        canvas.line((x, y), (x + length, y), 0.8, 1.0);
        canvas.line((x - 3.0, y + 3.0), (x + 3.0, y - 3.0), 0.8, 1.0);
        canvas.line(
            (x + length - 3.0, y + 3.0),
            (x + length + 3.0, y - 3.0),
            0.8,
            1.0,
        );
    }
    // Speckles.
    for _ in 0..(400.0 * area) as usize {
        let (x, y) = (random.range(0.0, w), random.range(0.0, h));
        canvas.line((x, y), (x + 0.5, y + 0.5), 1.2, random.range(0.3, 1.0));
    }
    for placement in placements {
        let margin = 0.25 * placement.side();
        canvas.clear(
            placement.x - margin,
            placement.y - margin,
            placement.side() + 2.0 * margin,
            placement.side() + 2.0 * margin,
        );
    }
    for &placement in placements {
        canvas.place(placement);
    }
    for pixel in &mut canvas.pixels {
        let noise = random.range(-6.0, 6.0);
        *pixel = (f32::from(*pixel) + noise).round().clamp(0.0, 255.0) as u8;
    }
    canvas.image()
}

fn placement(symbol: Symbol, x: f32, y: f32, turn: u8, size: f32) -> Placement {
    Placement {
        symbol,
        x,
        y,
        turn,
        size,
        mirrored: false,
    }
}

fn mirrored(placement: Placement) -> Placement {
    Placement {
        mirrored: true,
        ..placement
    }
}

/// A plan of 1000 × 700 px with every symbol at every turn and at 90 % and
/// 110 %. The first door is the source.
fn test_placements() -> Vec<Placement> {
    let symbols = [
        Symbol::Door,
        Symbol::CircleCross,
        Symbol::Arrow,
        Symbol::CircleSaltire,
    ];
    let mut placements = Vec::new();
    for (row, &symbol) in symbols.iter().enumerate() {
        // Fractions of a pixel change the anti-aliasing of each copy.
        let y = 60.45 + row as f32 * 150.15;
        for turn in 0..4u8 {
            let x = 60.0 + f32::from(turn) * 120.3;
            placements.push(placement(symbol, x, y + 0.25 * f32::from(turn), turn, 1.0));
        }
        placements.push(placement(symbol, 560.7, y + 10.0, 1, 0.9));
        placements.push(placement(symbol, 700.2, y + 0.3, 2, 1.1));
        placements.push(placement(symbol, 840.5, y + 40.6, 0, 1.1));
    }
    placements
}

/// The source box around a placement, in viewport points at scale 1.
fn box_around(placement: Placement) -> PdfRect {
    let margin = 5.0;
    PdfRect {
        x: f64::from(placement.x - margin),
        y: f64::from(placement.y - margin),
        width: f64::from(placement.side() + 2.0 * margin),
        height: f64::from(placement.side() + 2.0 * margin),
    }
}

fn centre_of(rect: PdfRect) -> (f32, f32) {
    (
        (rect.x + rect.width / 2.0) as f32,
        (rect.y + rect.height / 2.0) as f32,
    )
}

/// Where the centre of a hit's box for `target` must be: the template box
/// centre's offset from its symbol's centre, turned and scaled.
fn expected_centre(source: Placement, template_box: PdfRect, target: Placement) -> (f32, f32) {
    let (box_x, box_y) = centre_of(template_box);
    let (source_x, source_y) = source.centre();
    let (mut dx, mut dy) = (box_x - source_x, box_y - source_y);
    for _ in 0..target.turn {
        (dx, dy) = (-dy, dx);
    }
    let (target_x, target_y) = target.centre();
    let ratio = target.size / source.size;
    (target_x + dx * ratio, target_y + dy * ratio)
}

fn hit_near<H: HitSlice>(hits: &[H], centre: (f32, f32), reach: f32) -> Vec<&VisualHit> {
    hits.iter()
        .map(HitSlice::as_hit)
        .filter(|hit| {
            let (x, y) = centre_of(hit.viewport_rect);
            (x - centre.0).abs() <= reach && (y - centre.1).abs() <= reach
        })
        .collect()
}

struct Fixture {
    placements: Vec<Placement>,
    page: GreyImage,
}

fn fixture() -> Fixture {
    let placements = test_placements();
    let page = draw_page(1000, 700, &placements, 7);
    Fixture { placements, page }
}

fn template_for(fixture: &Fixture, symbol: Symbol) -> (Placement, VisualTemplate) {
    let source = *fixture
        .placements
        .iter()
        .find(|placement| {
            placement.symbol == symbol && placement.turn == 0 && placement.size == 1.0
        })
        .unwrap();
    let template = VisualTemplate::from_page_box(&fixture.page, 1.0, box_around(source)).unwrap();
    (source, template)
}

fn search(template: &PreparedTemplate, page: &GreyImage, contains_source: bool) -> Vec<VisualHit> {
    template
        .search_page(
            VisualPage {
                image: page,
                scale: 1.0,
                space: None,
                contains_source,
            },
            &AtomicBool::new(false),
        )
        .unwrap()
        .hits
}

fn check_symbol(symbol: Symbol, check_rotation: bool) {
    let fixture = fixture();
    let (source, template) = template_for(&fixture, symbol);
    let template_box = template.source_box().unwrap();
    let prepared = template
        .prepare(1.0, &VisualSearchOptions::default())
        .unwrap();
    let hits = search(&prepared, &fixture.page, false);
    let shown: Vec<&VisualHit> = hits
        .iter()
        .filter(|hit| hit.score >= DEFAULT_SIMILARITY)
        .collect();
    let mut report = Vec::new();
    for target in fixture
        .placements
        .iter()
        .filter(|placement| placement.symbol == symbol)
    {
        let centre = expected_centre(source, template_box, *target);
        // Weaker partial matches (a door turned to fit part of an arc) can
        // remain under the default similarity; only one hit is shown.
        let near = hit_near(&shown, centre, SYMBOL_PX);
        assert_eq!(
            near.len(),
            1,
            "{symbol:?} {target:?}: one shown hit, got {near:#?}"
        );
        let hit = near[0];
        let (x, y) = centre_of(hit.viewport_rect);
        report.push(format!(
            "{:?} turn {} size {:.2}: score {:.3}, found {}° {:.2}, offset ({:+.1}, {:+.1})",
            symbol,
            target.turn,
            target.size,
            hit.score,
            hit.rotation.degrees(),
            hit.size,
            x - centre.0,
            y - centre.1
        ));
        assert!(
            hit.score >= DEFAULT_SIMILARITY,
            "{}",
            report.last().unwrap()
        );
        let reach = if target.size == 1.0 { 1.5 } else { 2.0 };
        assert!(
            (x - centre.0).abs() <= reach && (y - centre.1).abs() <= reach,
            "{}",
            report.last().unwrap()
        );
        assert!(
            (hit.size - target.size).abs() <= 0.06,
            "{}",
            report.last().unwrap()
        );
        if check_rotation {
            assert_eq!(
                hit.rotation,
                target.rotation(),
                "{}",
                report.last().unwrap()
            );
            assert!(!hit.mirrored, "{}", report.last().unwrap());
        }
    }
    let expected = fixture
        .placements
        .iter()
        .filter(|placement| placement.symbol == symbol)
        .count();
    println!("{}", report.join("\n"));
    assert_eq!(
        shown.len(),
        expected,
        "{symbol:?}: hits at the default similarity: {shown:#?}"
    );
}

#[test]
fn finds_doors_at_every_turn_and_size() {
    check_symbol(Symbol::Door, true);
}

#[test]
fn finds_arrows_at_every_turn_and_size() {
    check_symbol(Symbol::Arrow, true);
}

#[test]
fn finds_a_symmetric_symbol_once_at_each_place() {
    // A circle with a cross looks the same at every turn: one turn is
    // enough, and each place gives one hit.
    check_symbol(Symbol::CircleCross, false);
    let fixture = fixture();
    let (_, template) = template_for(&fixture, Symbol::CircleCross);
    let prepared = template
        .prepare(1.0, &VisualSearchOptions::default())
        .unwrap();
    assert_eq!(prepared.variant_count(), 3, "{:?}", prepared.variants());
}

#[test]
fn finds_mirrored_doors_with_the_right_flip() {
    // A door at every turn, plain and mirrored, on the busy plan.
    let mut placements = Vec::new();
    for (row, flip) in [false, true].into_iter().enumerate() {
        for turn in 0..4u8 {
            let mut door = placement(
                Symbol::Door,
                60.3 + f32::from(turn) * 130.4,
                70.6 + row as f32 * 160.2 + 0.25 * f32::from(turn),
                turn,
                1.0,
            );
            door.mirrored = flip;
            placements.push(door);
        }
    }
    placements.push(mirrored(placement(Symbol::Door, 640.2, 400.5, 1, 1.1)));
    let page = draw_page(800, 560, &placements, 23);
    let source = placements[0];
    let template = VisualTemplate::from_page_box(&page, 1.0, box_around(source)).unwrap();
    let template_box = template.source_box().unwrap();
    let prepared = template
        .prepare(1.0, &VisualSearchOptions::default())
        .unwrap();
    assert_eq!(
        prepared.variant_count(),
        8 * 3,
        "a door differs at every turn and mirror: {:?}",
        prepared.variants()
    );
    let hits = search(&prepared, &page, false);
    let shown: Vec<&VisualHit> = hits
        .iter()
        .filter(|hit| hit.score >= DEFAULT_SIMILARITY)
        .collect();
    for target in &placements {
        // The box centre's offset from the symbol centre, mirrored first.
        let (box_x, box_y) = centre_of(template_box);
        let (source_x, source_y) = source.centre();
        let (mut dx, mut dy) = (box_x - source_x, box_y - source_y);
        if target.mirrored {
            dx = -dx;
        }
        for _ in 0..target.turn {
            (dx, dy) = (-dy, dx);
        }
        let (x, y) = target.centre();
        let centre = (x + dx * target.size, y + dy * target.size);
        let near = hit_near(&shown, centre, SYMBOL_PX);
        assert_eq!(near.len(), 1, "{target:?}: one hit, got {near:#?}");
        let hit = near[0];
        assert_eq!(hit.mirrored, target.mirrored, "{target:?}: {hit:?}");
        assert_eq!(hit.rotation, target.rotation(), "{target:?}: {hit:?}");
        assert!(
            (hit.size - target.size).abs() <= 0.06,
            "{target:?}: {hit:?}"
        );
        let (found_x, found_y) = centre_of(hit.viewport_rect);
        assert!(
            (found_x - centre.0).abs() <= 2.0 && (found_y - centre.1).abs() <= 2.0,
            "{target:?}: {hit:?}"
        );
    }
    assert_eq!(shown.len(), placements.len(), "{shown:#?}");

    // With the mirror off, the mirrored doors are not found.
    let plain = template
        .prepare(
            1.0,
            &VisualSearchOptions {
                mirrored: false,
                ..VisualSearchOptions::default()
            },
        )
        .unwrap();
    assert_eq!(plain.variant_count(), 4 * 3);
    let hits = search(&plain, &page, false);
    let found = hits
        .iter()
        .filter(|hit| hit.score >= DEFAULT_SIMILARITY)
        .count();
    assert_eq!(found, 4, "{hits:#?}");
}

#[test]
fn a_mirror_that_equals_a_turn_adds_no_variant() {
    let fixture = fixture();
    // An arrow flipped left to right is the arrow turned 180°.
    let (_, arrow) = template_for(&fixture, Symbol::Arrow);
    let options = VisualSearchOptions::default();
    assert!(options.mirrored, "mirrored copies are found by default");
    let prepared = arrow.prepare(1.0, &options).unwrap();
    assert_eq!(prepared.variant_count(), 4 * 3, "{:?}", prepared.variants());
    assert!(prepared.variants().iter().all(|variant| !variant.mirrored));
    // Without turns, the mirror is the one other picture.
    let no_turns = arrow
        .prepare(
            1.0,
            &VisualSearchOptions {
                turns: false,
                ..options.clone()
            },
        )
        .unwrap();
    assert_eq!(no_turns.variant_count(), 2 * 3, "{:?}", no_turns.variants());
    // A circle with a cross is the same picture in all eight.
    let (_, circle) = template_for(&fixture, Symbol::CircleCross);
    assert_eq!(circle.prepare(1.0, &options).unwrap().variant_count(), 3);
}

#[test]
fn a_near_miss_scores_below_the_default_similarity() {
    let fixture = fixture();
    let (_, template) = template_for(&fixture, Symbol::CircleCross);
    let options = VisualSearchOptions {
        min_score: 0.3,
        ..VisualSearchOptions::default()
    };
    let hits = search(
        &template.prepare(1.0, &options).unwrap(),
        &fixture.page,
        false,
    );
    let mut best = 0f32;
    for target in fixture
        .placements
        .iter()
        .filter(|placement| placement.symbol == Symbol::CircleSaltire)
    {
        for hit in hit_near(&hits, target.centre(), SYMBOL_PX / 2.0) {
            best = best.max(hit.score);
        }
    }
    println!("best near-miss score {best:.3}");
    assert!(best < DEFAULT_SIMILARITY, "near miss scored {best}");
}

#[test]
fn a_symbol_crossed_by_lines_is_found_at_a_lower_score() {
    // Doors sit in walls: a wall line runs through the hinge side of each
    // copy, and a grid line crosses one copy.
    let placements = vec![
        placement(Symbol::Door, 60.3, 60.4, 0, 1.0),
        placement(Symbol::Door, 200.6, 60.2, 0, 1.0),
        placement(Symbol::Door, 340.1, 60.7, 2, 1.0),
        placement(Symbol::Door, 480.4, 60.1, 1, 1.0),
    ];
    let mut canvas = Canvas::new(600, 200);
    for &placement in &placements {
        canvas.place(placement);
    }
    let wall_y = 60.4 + 0.9 * SYMBOL_PX;
    canvas.line((150.0, wall_y + 4.0), (440.0, wall_y + 4.0), 2.0, 1.0);
    canvas.line((150.0, wall_y + 11.0), (440.0, wall_y + 11.0), 2.0, 1.0);
    canvas.line((500.0, 0.0), (500.0, 200.0), 1.0, 0.45);
    let page = canvas.image();
    let source = placements[0];
    let template = VisualTemplate::from_page_box(&page, 1.0, box_around(source)).unwrap();
    let template_box = template.source_box().unwrap();
    let prepared = template
        .prepare(1.0, &VisualSearchOptions::default())
        .unwrap();
    let hits = search(&prepared, &page, false);
    for target in &placements[1..] {
        let centre = expected_centre(source, template_box, *target);
        let near = hit_near(&hits, centre, SYMBOL_PX / 2.0);
        assert_eq!(near.len(), 1, "{target:?}: {hits:#?}");
        let (x, y) = centre_of(near[0].viewport_rect);
        println!(
            "crossed {target:?}: score {:.3}, offset ({:+.1}, {:+.1})",
            near[0].score,
            x - centre.0,
            y - centre.1
        );
        assert_eq!(near[0].rotation, target.rotation());
        assert!((x - centre.0).abs() <= 1.5 && (y - centre.1).abs() <= 1.5);
    }
}

#[test]
fn the_threshold_drops_weaker_hits() {
    let fixture = fixture();
    let (_, template) = template_for(&fixture, Symbol::Door);
    let all = search(
        &template
            .prepare(1.0, &VisualSearchOptions::default())
            .unwrap(),
        &fixture.page,
        false,
    );
    let strict_options = VisualSearchOptions {
        min_score: 0.95,
        ..VisualSearchOptions::default()
    };
    let strict = search(
        &template.prepare(1.0, &strict_options).unwrap(),
        &fixture.page,
        false,
    );
    assert!(strict.iter().all(|hit| hit.score >= 0.95));
    assert!(
        !strict.is_empty() && strict.len() < all.len(),
        "{} of {}",
        strict.len(),
        all.len()
    );
    let expected = all.iter().filter(|hit| hit.score >= 0.95).count();
    assert_eq!(strict.len(), expected);
    let capped_options = VisualSearchOptions {
        max_hits: 3,
        ..VisualSearchOptions::default()
    };
    let capped = search(
        &template.prepare(1.0, &capped_options).unwrap(),
        &fixture.page,
        false,
    );
    assert_eq!(capped.len(), 3);
    assert_eq!(capped, all[..3].to_vec());
}

#[test]
fn hits_are_best_first_and_do_not_overlap() {
    let fixture = fixture();
    let (_, template) = template_for(&fixture, Symbol::Arrow);
    let hits = search(
        &template
            .prepare(1.0, &VisualSearchOptions::default())
            .unwrap(),
        &fixture.page,
        false,
    );
    assert!(hits.windows(2).all(|pair| pair[0].score >= pair[1].score));
    for (index, a) in hits.iter().enumerate() {
        for b in &hits[index + 1..] {
            let left = a.viewport_rect.x.max(b.viewport_rect.x);
            let right = a.viewport_rect.right().min(b.viewport_rect.right());
            let top = a.viewport_rect.y.max(b.viewport_rect.y);
            let bottom = a.viewport_rect.top().min(b.viewport_rect.top());
            let overlap = (right - left).max(0.0) * (bottom - top).max(0.0);
            let union = a.viewport_rect.width * a.viewport_rect.height
                + b.viewport_rect.width * b.viewport_rect.height
                - overlap;
            assert!(overlap / union < 0.3, "{a:?} overlaps {b:?}");
            let smaller = (a.viewport_rect.width * a.viewport_rect.height)
                .min(b.viewport_rect.width * b.viewport_rect.height);
            assert!(overlap / smaller < 0.7, "{b:?} is inside {a:?}");
        }
    }
}

#[test]
fn the_source_box_is_one_hit_marked_source() {
    let fixture = fixture();
    let (source, template) = template_for(&fixture, Symbol::Door);
    let prepared = template
        .prepare(1.0, &VisualSearchOptions::default())
        .unwrap();
    let hits = search(&prepared, &fixture.page, true);
    let sources: Vec<&VisualHit> = hits.iter().filter(|hit| hit.source).collect();
    assert_eq!(sources.len(), 1);
    let (x, y) = centre_of(sources[0].viewport_rect);
    let (box_x, box_y) = centre_of(template.source_box().unwrap());
    assert!((x - box_x).abs() <= 1.0 && (y - box_y).abs() <= 1.0);
    assert!(sources[0].score > 0.99, "{:?}", sources[0]);
    assert_eq!(hit_near(&hits, source.centre(), SYMBOL_PX).len(), 1);
    // On another page nothing is marked.
    assert!(
        search(&prepared, &fixture.page, false)
            .iter()
            .all(|hit| !hit.source)
    );
}

#[test]
fn hits_convert_to_pdf_space() {
    let fixture = fixture();
    let (_, template) = template_for(&fixture, Symbol::Door);
    let prepared = template
        .prepare(1.0, &VisualSearchOptions::default())
        .unwrap();
    let page_box = PdfRect::new(0.0, 0.0, 1000.0, 700.0).unwrap();
    let space = PageCoordinateSpace::new(page_box, page_box, Rotation::Degrees0, 1.0).unwrap();
    let matches = prepared
        .search_page(
            VisualPage {
                image: &fixture.page,
                scale: 1.0,
                space: Some(space),
                contains_source: false,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
    let hit = &matches.hits[0];
    let pdf = hit.pdf_rect.unwrap();
    assert!((pdf.x - hit.viewport_rect.x).abs() < 1e-9);
    assert!((pdf.y - (700.0 - hit.viewport_rect.top())).abs() < 1e-9);
    assert!((pdf.width - hit.viewport_rect.width).abs() < 1e-9);
}

#[test]
fn a_blank_or_uniform_template_is_refused() {
    let fixture = fixture();
    let blank = GreyImage::blank(200, 200);
    let area = PdfRect::new(10.0, 10.0, 60.0, 60.0).unwrap();
    assert_eq!(
        VisualTemplate::from_page_box(&blank, 1.0, area).unwrap_err(),
        VisualSearchError::TemplateEmpty
    );
    for grey in [0u8, 128, 200] {
        let uniform = GreyImage::new(80, 80, vec![grey; 6400]).unwrap();
        assert_eq!(
            VisualTemplate::from_picture(&uniform, 1.0).unwrap_err(),
            VisualSearchError::TemplateEmpty,
            "grey {grey}"
        );
    }
    // Noise alone is blank.
    let mut random = Random(3);
    let noise: Vec<u8> = (0..6400)
        .map(|_| (255.0 - random.range(0.0, 12.0)) as u8)
        .collect();
    assert_eq!(
        VisualTemplate::from_picture(&GreyImage::new(80, 80, noise).unwrap(), 1.0).unwrap_err(),
        VisualSearchError::TemplateEmpty
    );
    // A box with one short stroke is too small to find.
    let mut canvas = Canvas::new(80, 80);
    canvas.line((30.0, 30.0), (36.0, 30.0), 1.5, 1.0);
    assert_eq!(
        VisualTemplate::from_picture(&canvas.image(), 1.0).unwrap_err(),
        VisualSearchError::TemplateTooSmall
    );
    // A real template finds nothing on a blank or uniform page.
    let (_, template) = template_for(&fixture, Symbol::Door);
    let prepared = template
        .prepare(1.0, &VisualSearchOptions::default())
        .unwrap();
    assert!(search(&prepared, &GreyImage::blank(400, 300), false).is_empty());
    let grey_page = GreyImage::new(400, 300, vec![128; 120_000]).unwrap();
    assert!(search(&prepared, &grey_page, false).is_empty());
}

#[test]
fn a_pasted_picture_matches_at_its_screen_scale() {
    // A screenshot at 200 % zoom: 2 screen pixels for each point.
    let fixture = fixture();
    let mut canvas = Canvas::new(90, 90);
    canvas.place(Placement {
        symbol: Symbol::Arrow,
        x: 13.0,
        y: 13.0,
        turn: 0,
        size: 2.0,
        mirrored: false,
    });
    let template = VisualTemplate::from_picture(&canvas.image(), 2.0).unwrap();
    let options = VisualSearchOptions::for_pasted_picture();
    let prepared = template.prepare(1.0, &options).unwrap();
    assert_eq!(prepared.variant_count(), 4 * 10);
    let hits = search(&prepared, &fixture.page, false);
    for target in fixture
        .placements
        .iter()
        .filter(|placement| placement.symbol == Symbol::Arrow && placement.size == 1.0)
    {
        let near = hit_near(&hits, target.centre(), SYMBOL_PX / 2.0);
        assert_eq!(near.len(), 1, "{target:?}: {near:#?}");
        assert!(near[0].score >= DEFAULT_SIMILARITY, "{:?}", near[0]);
        assert_eq!(near[0].rotation, target.rotation());
        assert!((near[0].size - 1.0).abs() <= 0.1, "{:?}", near[0]);
    }
}

#[test]
fn a_cancel_stops_the_search_promptly() {
    let placements = test_placements();
    let page = draw_page(3000, 2000, &placements, 11);
    let fixture = Fixture {
        placements: placements.clone(),
        page,
    };
    let (_, template) = template_for(&fixture, Symbol::Door);
    let options = VisualSearchOptions {
        min_score: 0.3,
        ..VisualSearchOptions::default()
    };
    let prepared = Arc::new(template.prepare(1.0, &options).unwrap());

    // A cancel before the start ends at once.
    let cancelled = AtomicBool::new(true);
    let result = prepared.search_page(
        VisualPage {
            image: &fixture.page,
            scale: 1.0,
            space: None,
            contains_source: false,
        },
        &cancelled,
    );
    assert_eq!(result.unwrap_err(), VisualSearchError::Cancelled);

    // A cancel during the matching ends it within a short time.
    let page = Arc::new(PreparedPage::new(&fixture.page, 1.0, &AtomicBool::new(false)).unwrap());
    let cancel = Arc::new(AtomicBool::new(false));
    let worker = {
        let (prepared, page, cancel) = (prepared.clone(), page.clone(), cancel.clone());
        std::thread::spawn(move || {
            let result = prepared.search_prepared(&page, None, false, &cancel);
            (result, Instant::now())
        })
    };
    std::thread::sleep(Duration::from_millis(30));
    let cancelled_at = Instant::now();
    cancel.store(true, Ordering::Relaxed);
    let (result, ended_at) = worker.join().unwrap();
    let delay = ended_at.saturating_duration_since(cancelled_at);
    assert_eq!(result.unwrap_err(), VisualSearchError::Cancelled);
    println!("ended {delay:?} after the cancel");
    assert!(delay < Duration::from_millis(250), "{delay:?}");
}

#[test]
fn a_page_at_another_scale_is_refused() {
    let fixture = fixture();
    let (_, template) = template_for(&fixture, Symbol::Door);
    let prepared = template
        .prepare(1.0, &VisualSearchOptions::default())
        .unwrap();
    let page = PreparedPage::new(&fixture.page, 1.41, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        prepared
            .search_prepared(&page, None, false, &AtomicBool::new(false))
            .unwrap_err(),
        VisualSearchError::ScaleMismatch
    );
}

fn time<T>(runs: usize, mut work: impl FnMut() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut last = None;
    for _ in 0..runs {
        let started = Instant::now();
        let value = work();
        best = best.min(started.elapsed());
        last = Some(value);
    }
    (best, last.unwrap())
}

/// A synthetic A1 sheet (2384 × 1684 pt) at the design's working scale for a
/// 10 mm symbol (1 px per point): 4.0 Mpx. It prints the time for each
/// variant, the total for 12 variants and the recall and precision.
#[test]
#[ignore = "benchmark: run in an optimised build with --nocapture"]
fn benchmark_a1_sheet_with_a_10_mm_symbol() {
    let scale = butter_paper::visual_search::working_scale(28.35, (2384.0, 1684.0));
    assert_eq!(scale, 1.0);
    let (width, height) = (2384, 1684);
    let side = 28.35 / SYMBOL_PX;
    let mut random = Random(99);
    let mut placements = Vec::new();
    let symbols = [
        Symbol::Door,
        Symbol::Arrow,
        Symbol::CircleCross,
        Symbol::CircleSaltire,
    ];
    for row in 0..10 {
        for column in 0..12 {
            if random.unit() < 0.5 {
                continue;
            }
            let symbol = symbols[(random.next() % 4) as usize];
            let size = side * [0.9, 1.0, 1.0, 1.1][(random.next() % 4) as usize];
            placements.push(placement(
                symbol,
                60.0 + column as f32 * 190.0 + random.range(0.0, 60.0),
                50.0 + row as f32 * 160.0 + random.range(0.0, 60.0),
                (random.next() % 4) as u8,
                size,
            ));
        }
    }
    // Make sure that one door is at 0° and 100 % for the source.
    placements.push(placement(Symbol::Door, 20.0, 20.0, 0, side));
    let draw_started = Instant::now();
    let page = draw_page(width, height, &placements, 5);
    println!("drew {width} × {height} px in {:?}", draw_started.elapsed());
    let source = *placements.last().unwrap();
    let template = VisualTemplate::from_page_box(&page, scale, box_around(source)).unwrap();
    println!(
        "template {} × {} px (trimmed), {} placements",
        template.width_px(),
        template.height_px(),
        placements.len()
    );

    let (prepare_template, prepared) = time(5, || {
        template
            .prepare(scale, &VisualSearchOptions::default())
            .unwrap()
    });
    println!(
        "template prepare: {prepare_template:?}, {} variants, {} bytes",
        prepared.variant_count(),
        prepared.memory_bytes()
    );
    let cancel = AtomicBool::new(false);
    let (prepare_page, prepared_page) =
        time(5, || PreparedPage::new(&page, scale, &cancel).unwrap());
    println!(
        "page prepare (ink, blur, pyramid): {prepare_page:?}, {} bytes (input grey {} bytes)",
        prepared_page.memory_bytes(),
        width * height
    );

    for (index, info) in prepared.variants().iter().enumerate() {
        let single = prepared.only_variants(|kept| kept == index);
        let (elapsed, matches) = time(3, || {
            single
                .search_prepared(&prepared_page, None, false, &cancel)
                .unwrap()
        });
        println!(
            "variant {index:2}: {:3}° size {:.2} {:2}×{:2} px level {}: {elapsed:?}, {} positions, {:.1} % correlated, {} candidates",
            info.rotation.degrees(),
            info.size,
            info.width_px,
            info.height_px,
            info.coarse_level,
            matches.stats.positions,
            100.0 * matches.stats.correlated as f64 / matches.stats.positions.max(1) as f64,
            matches.stats.candidates,
        );
    }
    let (all, matches) = time(3, || {
        prepared
            .search_prepared(&prepared_page, None, false, &cancel)
            .unwrap()
    });
    println!(
        "all {} variants: {all:?} matching + {prepare_page:?} page prepare = {:?} per page; working bytes {}",
        prepared.variant_count(),
        all + prepare_page,
        matches.stats.working_bytes
    );

    let template_box = template.source_box().unwrap();
    let doors: Vec<Placement> = placements
        .iter()
        .copied()
        .filter(|placement| placement.symbol == Symbol::Door)
        .collect();
    let shown: Vec<&VisualHit> = matches
        .hits
        .iter()
        .filter(|hit| hit.score >= DEFAULT_SIMILARITY)
        .collect();
    let mut found = 0;
    let mut worst_offset = 0f32;
    let mut lowest = 1f32;
    for door in &doors {
        let centre = expected_centre(source, template_box, *door);
        if let Some(hit) = hit_near(&shown, centre, 10.0).into_iter().next() {
            found += 1;
            let (x, y) = centre_of(hit.viewport_rect);
            worst_offset = worst_offset.max((x - centre.0).abs().max((y - centre.1).abs()));
            lowest = lowest.min(hit.score);
        } else {
            println!("missed {door:?}");
        }
    }
    println!(
        "recall {found}/{} doors at {DEFAULT_SIMILARITY}, precision {found}/{} hits, worst centre offset {worst_offset:.1} px, lowest true score {lowest:.3}",
        doors.len(),
        shown.len()
    );
}

/// Keeps `hit_near` usable with a slice of references.
trait HitSlice {
    fn as_hit(&self) -> &VisualHit;
}

impl HitSlice for VisualHit {
    fn as_hit(&self) -> &VisualHit {
        self
    }
}

impl HitSlice for &VisualHit {
    fn as_hit(&self) -> &VisualHit {
        self
    }
}

// The run: pages drawn by documents and matched on a thread pool.

mod run {
    use super::*;
    use butter_paper::visual_search_run::{
        SharedVisualRenderCache, TemplateSource, VisualPageRender, VisualResults,
        VisualSearchDocument, VisualSearchEvent, VisualSearchRun,
    };
    use std::sync::Mutex;

    /// A document of pages drawn with `draw_page`, which counts its draws.
    struct PagesDocument {
        id: u64,
        pages: Vec<Arc<GreyImage>>,
        start: u32,
        delay: Duration,
        drawn: Mutex<Vec<u32>>,
    }

    impl PagesDocument {
        fn new(id: u64, pages: Vec<Arc<GreyImage>>) -> Self {
            Self {
                id,
                pages,
                start: 0,
                delay: Duration::ZERO,
                drawn: Mutex::default(),
            }
        }
    }

    impl VisualSearchDocument for PagesDocument {
        fn id(&self) -> u64 {
            self.id
        }
        fn page_count(&self) -> u32 {
            self.pages.len() as u32
        }
        fn start_page(&self) -> u32 {
            self.start
        }
        fn render_page(&self, page_index: u32, scale: f32) -> Result<VisualPageRender, String> {
            assert_eq!(scale, 1.0, "pages are drawn at the template's scale");
            std::thread::sleep(self.delay);
            self.drawn.lock().unwrap().push(page_index);
            Ok(VisualPageRender {
                image: self.pages[page_index as usize].clone(),
                space: None,
            })
        }
    }

    /// A one-page document that shares its page picture with the others,
    /// so the picture's reference count tells how many drawn pages are in
    /// memory.
    struct SharedPageDocument {
        id: u64,
        page: Arc<GreyImage>,
        /// References that are not drawn pages: one for each document and
        /// one for the test.
        held: usize,
        most_drawn: Arc<AtomicUsize>,
    }

    impl VisualSearchDocument for SharedPageDocument {
        fn id(&self) -> u64 {
            self.id
        }
        fn page_count(&self) -> u32 {
            1
        }
        fn render_page(&self, _: u32, _: f32) -> Result<VisualPageRender, String> {
            let image = self.page.clone();
            let drawn = Arc::strong_count(&image) - self.held;
            self.most_drawn.fetch_max(drawn, Ordering::AcqRel);
            std::thread::sleep(Duration::from_millis(5));
            Ok(VisualPageRender { image, space: None })
        }
    }

    /// Each document has its own thread, but the drawn pages in memory are
    /// limited by the matcher count, not the document count.
    #[test]
    fn many_documents_keep_few_drawn_pages_in_memory() {
        const DOCUMENTS: usize = 30;
        const MATCHERS: usize = 2;
        let fixture = fixture();
        let (_, template) = template_for(&fixture, Symbol::Door);
        let prepared = Arc::new(
            template
                .prepare(1.0, &VisualSearchOptions::default())
                .unwrap(),
        );
        let page = Arc::new(fixture.page.clone());
        let most_drawn = Arc::new(AtomicUsize::new(0));
        let documents: Vec<Arc<dyn VisualSearchDocument>> = (0..DOCUMENTS as u64)
            .map(|id| {
                Arc::new(SharedPageDocument {
                    id,
                    page: page.clone(),
                    held: DOCUMENTS + 1,
                    most_drawn: most_drawn.clone(),
                }) as Arc<dyn VisualSearchDocument>
            })
            .collect();
        let finished = Arc::new(AtomicUsize::new(0));
        let sink = finished.clone();
        let run = VisualSearchRun::start(documents, prepared, None, MATCHERS, move |event| {
            if matches!(event, VisualSearchEvent::Finished { .. }) {
                sink.fetch_add(1, Ordering::AcqRel);
            }
        });
        run.join();
        assert_eq!(finished.load(Ordering::Acquire), 1);
        // A slot for each matcher, the queue (one page for each matcher) and
        // the pages being matched.
        let most = most_drawn.load(Ordering::Acquire);
        assert!(
            most <= 3 * MATCHERS,
            "{most} drawn pages in memory at one time for {MATCHERS} matchers"
        );
    }

    fn run_to_end(
        documents: Vec<Arc<dyn VisualSearchDocument>>,
        template: Arc<PreparedTemplate>,
        source: Option<TemplateSource>,
        names: &[(u64, &str, u32)],
    ) -> (Vec<VisualSearchEvent>, VisualResults) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let run = VisualSearchRun::start(documents, template, source, 3, move |event| {
            sink.lock().unwrap().push(event);
        });
        run.join();
        let events = events.lock().unwrap().clone();
        let mut results = VisualResults::new(
            names
                .iter()
                .map(|(id, name, pages)| (*id, (*name).to_owned(), *pages)),
        );
        for event in events.iter().cloned() {
            results.apply(event);
        }
        (events, results)
    }

    /// Two documents: the fixture page twice in the first, and a page with
    /// two doors then a blank page in the second.
    fn documents() -> (Fixture, Arc<PagesDocument>, Arc<PagesDocument>) {
        let fixture = fixture();
        let page = Arc::new(fixture.page.clone());
        let doors = vec![
            placement(Symbol::Door, 100.4, 80.2, 1, 1.0),
            mirrored(placement(Symbol::Door, 300.7, 200.3, 0, 1.0)),
        ];
        let second = Arc::new(draw_page(500, 400, &doors, 3));
        let first = Arc::new(PagesDocument::new(1, vec![page.clone(), page]));
        let other = Arc::new(PagesDocument::new(
            2,
            vec![second, Arc::new(GreyImage::blank(500, 400))],
        ));
        (fixture, first, other)
    }

    #[test]
    fn a_run_reports_hits_by_document_and_page_with_the_source_marked() {
        let (fixture, first, other) = documents();
        let (_, template) = template_for(&fixture, Symbol::Door);
        let prepared = Arc::new(
            template
                .prepare(1.0, &VisualSearchOptions::default())
                .unwrap(),
        );
        let source = TemplateSource {
            document_id: 1,
            page_index: 1,
        };
        let (events, results) = run_to_end(
            vec![first.clone(), other.clone()],
            prepared,
            Some(source),
            &[(1, "plan.pdf", 2), (2, "other.pdf", 2)],
        );
        assert_eq!(
            events.last(),
            Some(&VisualSearchEvent::Finished { cancelled: false })
        );
        assert!(events.contains(&VisualSearchEvent::DocumentDone { document_id: 1 }));
        assert!(events.contains(&VisualSearchEvent::DocumentDone { document_id: 2 }));
        assert_eq!(results.progress(), (4, 4));
        let doors = fixture
            .placements
            .iter()
            .filter(|placement| placement.symbol == Symbol::Door)
            .count();
        let documents = results.documents();
        assert_eq!(documents[0].hit_count(DEFAULT_SIMILARITY), 2 * doors);
        assert_eq!(documents[1].hit_count(DEFAULT_SIMILARITY), 2, "both doors");
        // The source is marked on its page only.
        let sources = results
            .shown_keys(DEFAULT_SIMILARITY)
            .into_iter()
            .filter(|key| results.hit(*key).unwrap().hit.source)
            .collect::<Vec<_>>();
        assert_eq!(sources.len(), 1);
        assert_eq!((sources[0].0, sources[0].1), (1, 1));
        // Keys go by document, then page, then best first.
        let keys = results.shown_keys(DEFAULT_SIMILARITY);
        assert!(
            keys.windows(2)
                .all(|pair| (pair[0].0, pair[0].1) <= (pair[1].0, pair[1].1))
        );
        for pair in keys.windows(2) {
            if (pair[0].0, pair[0].1) == (pair[1].0, pair[1].1) {
                let (a, b) = (results.hit(pair[0]).unwrap(), results.hit(pair[1]).unwrap());
                assert!(a.hit.score >= b.hit.score);
            }
        }
        // The mirrored door is found mirrored.
        let other_hits = results.page_hits(2, 0);
        assert_eq!(other_hits.iter().filter(|hit| hit.hit.mirrored).count(), 1);
        // Each hit has its picture.
        assert!(
            keys.iter()
                .all(|key| results.hit(*key).unwrap().tile.width <= 96)
        );
        // The source document starts at the source page when asked; this
        // one started at page 0 and drew both.
        assert_eq!(first.drawn.lock().unwrap().len(), 2);
    }

    #[test]
    fn the_threshold_filters_what_was_found_without_a_new_search() {
        let (fixture, first, _) = documents();
        let (_, template) = template_for(&fixture, Symbol::CircleCross);
        let prepared = Arc::new(
            template
                .prepare(1.0, &VisualSearchOptions::default())
                .unwrap(),
        );
        let (_, results) = run_to_end(
            vec![first.clone()],
            prepared,
            Some(TemplateSource {
                document_id: 1,
                page_index: 0,
            }),
            &[(1, "plan.pdf", 2)],
        );
        let loose = results.hit_count(0.60);
        let default = results.hit_count(DEFAULT_SIMILARITY);
        let strict = results.hit_count(0.99);
        assert!(
            loose >= default && default > strict,
            "{loose} {default} {strict}"
        );
        // The source shows at any similarity.
        assert!(strict >= 1);
        assert!(
            results
                .shown_keys(0.99)
                .iter()
                .any(|key| results.hit(*key).unwrap().hit.source)
        );
        // The filter needed no new drawing.
        assert_eq!(first.drawn.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_cancel_ends_the_run_within_a_page() {
        let fixture = fixture();
        let page = Arc::new(fixture.page.clone());
        let mut document = PagesDocument::new(1, vec![page; 40]);
        document.delay = Duration::from_millis(15);
        let document = Arc::new(document);
        let (_, template) = template_for(&fixture, Symbol::Arrow);
        let prepared = Arc::new(
            template
                .prepare(1.0, &VisualSearchOptions::default())
                .unwrap(),
        );
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let run = VisualSearchRun::start(
            vec![document.clone() as Arc<dyn VisualSearchDocument>],
            prepared,
            None,
            2,
            move |event| sink.lock().unwrap().push(event),
        );
        std::thread::sleep(Duration::from_millis(60));
        run.cancel();
        let cancelled_at = Instant::now();
        run.join();
        assert!(cancelled_at.elapsed() < Duration::from_millis(500));
        let drawn = document.drawn.lock().unwrap().len();
        assert!(drawn < 40, "stopped early: {drawn}");
        let events = events.lock().unwrap();
        assert_eq!(
            events.last(),
            Some(&VisualSearchEvent::Finished { cancelled: true })
        );
        assert!(!events.contains(&VisualSearchEvent::DocumentDone { document_id: 1 }));
    }

    #[test]
    fn a_closed_document_is_no_longer_drawn_and_the_others_go_on() {
        let fixture = fixture();
        let page = Arc::new(fixture.page.clone());
        let mut closing = PagesDocument::new(1, vec![page.clone(); 30]);
        closing.delay = Duration::from_millis(10);
        let closing = Arc::new(closing);
        let staying = Arc::new(PagesDocument::new(2, vec![page; 3]));
        let (_, template) = template_for(&fixture, Symbol::Arrow);
        let prepared = Arc::new(
            template
                .prepare(1.0, &VisualSearchOptions::default())
                .unwrap(),
        );
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let run = VisualSearchRun::start(
            vec![
                closing.clone() as Arc<dyn VisualSearchDocument>,
                staying.clone(),
            ],
            prepared,
            None,
            2,
            move |event| sink.lock().unwrap().push(event),
        );
        std::thread::sleep(Duration::from_millis(40));
        run.cancel_document(1);
        run.join();
        assert!(closing.drawn.lock().unwrap().len() < 30);
        assert_eq!(staying.drawn.lock().unwrap().len(), 3);
        let events = events.lock().unwrap();
        assert!(events.contains(&VisualSearchEvent::DocumentDone { document_id: 2 }));
        assert_eq!(
            events.last(),
            Some(&VisualSearchEvent::Finished { cancelled: false })
        );
    }

    #[test]
    fn the_render_cache_keeps_a_page_for_its_state_and_scale() {
        let cache = SharedVisualRenderCache::with_budget(3 * 100 * 100);
        let render = || VisualPageRender {
            image: Arc::new(GreyImage::blank(100, 100)),
            space: None,
        };
        let mut locked = cache.lock();
        locked.insert(1, 0, 7, 1.0, render());
        assert!(locked.get(1, 0, 7, 1.0).is_some());
        assert!(locked.get(1, 0, 8, 1.0).is_none(), "the page changed");
        assert!(locked.get(1, 0, 7, 1.41).is_none(), "another scale");
        locked.insert(1, 1, 7, 1.0, render());
        locked.insert(2, 0, 7, 1.0, render());
        // Over budget: the least recently used page goes.
        let _ = locked.get(1, 0, 7, 1.0);
        locked.insert(2, 1, 7, 1.0, render());
        assert_eq!(locked.len(), 3);
        assert!(locked.get(1, 1, 7, 1.0).is_none());
        assert!(locked.get(1, 0, 7, 1.0).is_some());
        locked.remove_document(2);
        assert_eq!(locked.len(), 1);
        assert_eq!(locked.bytes(), 100 * 100);
    }
}
