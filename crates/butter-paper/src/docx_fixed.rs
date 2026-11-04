//! Word pages that keep the PDF's layout, for pages flowing paragraphs
//! cannot hold: drawings, plans, maps, posters and slides, whose text sits
//! at places on the page and over graphics. The page's drawings and images
//! are pictures behind the text, in the PDF's paint order (paths as SVG,
//! see [`crate::pdf_page_graphics`]); each line of text is a text box of
//! its own, borderless and unwrapped, at the line's place and angle. Words
//! the PDF sets apart stay apart at tab stops, and each stretch of text is
//! spaced to the width the PDF gives it, measured in the metric-compatible
//! font Word draws it in (Arial, Times New Roman or a monospace). The text
//! stays text, to edit, search and copy; fonts the reader is unlikely to
//! have are named by the common family of their kind.

use std::fmt::Write as _;
use std::sync::OnceLock;

use crate::pdf_page_graphics::{GraphicsLayer, PageGraphics, PlacedImage};
use crate::pdf_text_edit::{FallbackFamily, PageTextLayer, PageTextLine};

/// Points to Word's twentieths of a point.
fn twips(points: f64) -> i64 {
    (points * 20.).round() as i64
}

/// Points to English Metric Units.
fn emu(points: f64) -> i64 {
    (points * 12_700.).round() as i64
}

/// The largest page side Word allows, in points (22 inches): it shrinks a
/// larger page's sides to it, cutting off what lies beyond.
const WORD_LARGEST_SIDE: f64 = 1584.;

/// How much a page of box `page_box` is scaled to fit Word's largest page:
/// a drawing sheet larger than 22 inches (A1, A0) keeps its proportions at
/// a smaller size, as printing it smaller would.
pub(crate) fn page_scale(page_box: [f64; 4]) -> f64 {
    let largest = (page_box[2] - page_box[0]).max(page_box[3] - page_box[1]);
    if largest > WORD_LARGEST_SIDE {
        WORD_LARGEST_SIDE / largest
    } else {
        1.
    }
}

/// A page whose layout is kept.
pub(crate) struct FixedPage {
    /// What the page is scaled by to fit Word ([`page_scale`]).
    pub scale: f64,
    pub graphics: PageGraphics,
    pub lines: Vec<FixedLine>,
    /// Into the document's media: each SVG layer's and each picture's, in
    /// layer order, filled in when the document's media are gathered.
    pub svg_media: Vec<usize>,
    pub picture_media: Vec<usize>,
}

/// A line of text in its own box.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FixedLine {
    /// The box before rotation, in points from the page's top left.
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    /// Clockwise, in degrees.
    pub rotation: f64,
    /// Exact line height, in points.
    pub line_height: f64,
    pub segments: Vec<Segment>,
    /// Graphics painted after it hide it: it lies behind them, as on the
    /// PDF's page.
    pub hidden: bool,
    /// A line turned a quarter: written vertically (`vert270` reads upward,
    /// `vert` downward) in an upright box, which every word processor draws.
    pub vertical: Option<&'static str>,
}

/// Text the PDF places apart from the text before it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Segment {
    /// From the box's left edge, in points.
    pub offset: f64,
    pub runs: Vec<FixedRun>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FixedRun {
    pub text: String,
    pub font: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub colour: Option<[u8; 3]>,
    /// Extra space after each character, in twentieths of a point.
    pub spacing: i64,
    /// Horizontal scale, in percent.
    pub scale: u32,
}

/// Whether page text is better kept in place than flowed: the page is a
/// large sheet, its text is drawn at an angle, the page draws more than
/// rules and boxes (a drawing, a map), or an image lies under its text.
pub(crate) fn keeps_layout(layer: &PageTextLayer, graphics: &PageGraphics) -> bool {
    // A sheet larger than A3 (a drawing, a poster) is laid out, not
    // written to flow.
    if let Some([left, bottom, right, top]) = layer.page_box
        && (right - left).max(top - bottom) > 1250.
    {
        return true;
    }
    if layer.lines.iter().any(|line| {
        let m = line.matrix;
        m[1].abs() > 1e-3 * m[0].abs().max(1e-9) || m[2].abs() > 1e-3 * m[3].abs().max(1e-9)
    }) {
        return true;
    }
    if graphics.shapes > 40 {
        return true;
    }
    let text_boxes = layer
        .lines
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .filter_map(line_bounds)
        .collect::<Vec<_>>();
    graphics.layers.iter().any(|layer| match layer {
        GraphicsLayer::Picture(image) => {
            let Some([l, b, r, t]) = bounds(&image.shown_corners()) else {
                return false;
            };
            text_boxes
                .iter()
                .any(|[tl, tb, tr, tt]| tl < &r && tr > &l && tb < &t && tt > &b)
        }
        GraphicsLayer::Svg(_) => false,
    }) || graphics.embedded_images > 0
}

fn bounds(points: &[(f64, f64)]) -> Option<[f64; 4]> {
    let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for (x, y) in points {
        bounds = [
            bounds[0].min(*x),
            bounds[1].min(*y),
            bounds[2].max(*x),
            bounds[3].max(*y),
        ];
    }
    (bounds[0] <= bounds[2]).then_some(bounds)
}

/// A line's bounds on the page (left, bottom, right, top).
fn line_bounds(line: &PageTextLine) -> Option<[f64; 4]> {
    let first = line.chars.first()?;
    let m = line.matrix;
    let size = line
        .styles
        .iter()
        .map(|style| style.font_size.abs())
        .fold(0., f64::max);
    let corners = [
        (first.x, -0.2 * size),
        (line.end_x, -0.2 * size),
        (line.end_x, 0.8 * size),
        (first.x, 0.8 * size),
    ]
    .map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
    let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for (x, y) in corners {
        bounds = [
            bounds[0].min(x),
            bounds[1].min(y),
            bounds[2].max(x),
            bounds[3].max(y),
        ];
    }
    Some(bounds)
}

// --- Fonts -----------------------------------------------------------------

/// Families a reader is likely to have, or that Word substitutes well.
const COMMON_FAMILIES: &[&str] = &[
    "Arial",
    "Arial Narrow",
    "Arial Black",
    "Helvetica",
    "Helvetica Neue",
    "Times New Roman",
    "Times",
    "Courier New",
    "Courier",
    "Calibri",
    "Calibri Light",
    "Cambria",
    "Candara",
    "Consolas",
    "Constantia",
    "Corbel",
    "Georgia",
    "Verdana",
    "Tahoma",
    "Trebuchet MS",
    "Segoe UI",
    "Garamond",
    "Book Antiqua",
    "Century Gothic",
    "Gill Sans MT",
    "Franklin Gothic Medium",
    "Lucida Console",
    "Lucida Sans Unicode",
    "Palatino Linotype",
    "Symbol",
    "Wingdings",
    "Aptos",
    "Liberation Sans",
    "Liberation Serif",
    "Liberation Mono",
];

/// The metric-compatible face text in `family` is measured with, if any.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Metrics {
    Sans,
    Serif,
    Mono,
}

fn metrics_for(family: &str) -> Option<Metrics> {
    match family {
        "Arial" | "Helvetica" | "Liberation Sans" | "Arimo" => Some(Metrics::Sans),
        "Times New Roman" | "Times" | "Liberation Serif" | "Tinos" => Some(Metrics::Serif),
        "Courier New" | "Courier" | "Liberation Mono" | "Cousine" => Some(Metrics::Mono),
        _ => None,
    }
}

/// The family Word draws a PDF font in: its own when common, otherwise the
/// common family of its kind.
pub(crate) fn word_family(named: &str, fallback: FallbackFamily) -> String {
    if COMMON_FAMILIES
        .iter()
        .any(|common| common.eq_ignore_ascii_case(named))
    {
        return named.to_owned();
    }
    let lower = named.to_ascii_lowercase();
    // Families sold under other names that match a common one.
    let sans = [
        "swiss",
        "helvet",
        "arimo",
        "nimbus sans",
        "nimbussans",
        "liberation sans",
    ];
    let serif = ["dutch", "tinos", "nimbus roman", "nimbusrom", "times"];
    let mono = ["cousine", "nimbus mono", "nimbusmon", "courier"];
    if sans.iter().any(|name| lower.contains(name)) {
        return "Arial".into();
    }
    if serif.iter().any(|name| lower.contains(name)) {
        return "Times New Roman".into();
    }
    if mono.iter().any(|name| lower.contains(name)) {
        return "Courier New".into();
    }
    match fallback {
        FallbackFamily::Sans => "Arial",
        FallbackFamily::Serif => "Times New Roman",
        FallbackFamily::Mono => "Courier New",
    }
    .into()
}

struct Face {
    face: ttf_parser::Face<'static>,
    units: f64,
}

fn face(metrics: Metrics, bold: bool, italic: bool) -> &'static Face {
    use crate::pdf_engine::EmbeddedTextFont as Font;
    static FACES: OnceLock<Vec<Face>> = OnceLock::new();
    let faces = FACES.get_or_init(|| {
        [
            Font::ArimoRegular,
            Font::ArimoBold,
            Font::ArimoItalic,
            Font::ArimoBoldItalic,
            Font::TinosRegular,
            Font::TinosBold,
            Font::TinosItalic,
            Font::TinosBoldItalic,
            Font::RobotoMonoRegular,
            Font::RobotoMonoBold,
            Font::RobotoMonoItalic,
            Font::RobotoMonoBoldItalic,
        ]
        .into_iter()
        .map(|font| {
            let face = ttf_parser::Face::parse(font.data(), 0).expect("bundled font");
            Face {
                units: f64::from(face.units_per_em()),
                face,
            }
        })
        .collect()
    });
    let family = match metrics {
        Metrics::Sans => 0,
        Metrics::Serif => 4,
        Metrics::Mono => 8,
    };
    &faces[family + usize::from(bold) + 2 * usize::from(italic)]
}

impl Face {
    /// A character's advance, in ems.
    fn advance(&self, character: char) -> f64 {
        let glyph = self
            .face
            .glyph_index(character)
            .or_else(|| self.face.glyph_index('?'));
        glyph
            .and_then(|glyph| self.face.glyph_hor_advance(glyph))
            .map_or(0.5, |advance| f64::from(advance) / self.units)
    }

    /// Above and below the baseline, in ems.
    fn extent(&self) -> (f64, f64) {
        let hhea_ascent = f64::from(self.face.ascender()) / self.units;
        let hhea_descent = -f64::from(self.face.descender()) / self.units;
        (hhea_ascent, hhea_descent)
    }
}

/// Sans-serif ascent and descent in ems, as Arial's.
fn default_extent() -> (f64, f64) {
    face(Metrics::Sans, false, false).extent()
}

// --- Lines -----------------------------------------------------------------

/// The reason a line's text is not drawn (`Tr 3`, or clipping only).
pub(crate) const INVISIBLE_TEXT: &str = "invisible or clipping text is not editable";

/// Whether a line draws nothing a reader sees.
pub(crate) fn invisible(line: &PageTextLine) -> bool {
    line.read_only.as_deref() == Some(INVISIBLE_TEXT)
}

/// The page's visible text as boxes of lines on page `page_box`.
pub(crate) fn fixed_lines(
    layer: &PageTextLayer,
    page_box: [f64; 4],
    graphics: &PageGraphics,
    scale: f64,
) -> Vec<FixedLine> {
    layer
        .lines
        .iter()
        .filter(|line| !invisible(line) && !line.text.trim().is_empty())
        .filter(|line| {
            // A line its clip hides entirely is not seen.
            match (line.clip_bounds, line_bounds(line)) {
                (Some([cl, cb, cr, ct]), Some([l, b, r, t])) => {
                    r > cl && l < cr && t > cb && b < ct
                }
                _ => true,
            }
        })
        .filter_map(|line| fixed_line(layer, line, page_box, graphics, scale))
        .map(|line| line.scaled(scale))
        .collect()
}

impl FixedLine {
    /// The line on a page scaled by `scale`.
    fn scaled(mut self, scale: f64) -> Self {
        if scale == 1. {
            return self;
        }
        self.left *= scale;
        self.top *= scale;
        self.width *= scale;
        self.height *= scale;
        self.line_height *= scale;
        for segment in &mut self.segments {
            segment.offset *= scale;
            for run in &mut segment.runs {
                run.size *= scale;
                run.spacing = (run.spacing as f64 * scale).round() as i64;
            }
        }
        self
    }
}

fn fixed_line(
    layer: &PageTextLayer,
    line: &PageTextLine,
    page_box: [f64; 4],
    graphics: &PageGraphics,
    scale: f64,
) -> Option<FixedLine> {
    let first = line.chars.first()?;
    let m = line.matrix;
    // Along the baseline and up from it, in page space, per text unit.
    let along = m[0].hypot(m[1]);
    let up = m[2].hypot(m[3]);
    if along < 1e-9 || up < 1e-9 {
        return None;
    }
    let angle = m[1].atan2(m[0]);
    let (cos, sin) = (angle.cos(), angle.sin());
    // Gaps wider than this start a new segment, at a tab stop.
    let mut segments: Vec<(f64, Vec<(usize, &crate::pdf_text_edit::TextLineChar)>)> = Vec::new();
    let mut previous_end: Option<f64> = None;
    for (index, character) in line.chars.iter().enumerate() {
        let size = line
            .styles
            .get(character.style)
            .map_or(10., |style| style.font_size.abs());
        let gap = previous_end.map(|end| character.x - end);
        let starts = match gap {
            None => true,
            Some(gap) => gap > 0.6 * size || gap < -0.3 * size,
        };
        // A synthetic space is a gap read as a space: it starts a segment
        // when wide, and is otherwise kept as the space it stands for.
        if character.synthetic && character.advance > 0.6 * size {
            previous_end = Some(character.x);
            continue;
        }
        if starts {
            segments.push((character.x, Vec::new()));
        }
        segments
            .last_mut()
            .expect("a segment")
            .1
            .push((index, character));
        previous_end = Some(character.x + character.advance);
    }
    let segments = segments
        .into_iter()
        .filter(|(_, chars)| chars.iter().any(|(_, c)| !c.character.is_whitespace()))
        .collect::<Vec<_>>();
    let start_x = segments.first()?.0;
    let mut max_size: f64 = 0.;
    let mut extent: (f64, f64) = default_extent();
    let mut built = Vec::new();
    for (segment_x, chars) in &segments {
        let mut runs: Vec<FixedRun> = Vec::new();
        let mut natural = 0.;
        let mut measured = true;
        for (_, character) in chars {
            let Some(style) = line.styles.get(character.style) else {
                continue;
            };
            let run = run_style(layer, line, character.style, up, scale);
            max_size = max_size.max(run.size);
            match metrics_for(&run.font) {
                Some(metrics) => {
                    let face = face(metrics, run.bold, run.italic);
                    extent = face.extent();
                    natural += face.advance(character.character) * run.size;
                }
                None => measured = false,
            }
            let _ = style;
            match runs.last_mut() {
                Some(last) if same_style(last, &run) => last.text.push(character.character),
                _ => {
                    let mut run = run;
                    run.text.push(character.character);
                    runs.push(run);
                }
            }
        }
        let (_, last) = chars.last().expect("not empty");
        let pdf_width = (last.x + last.advance - segment_x) * along;
        let count = chars.len().max(1) as f64;
        if measured && natural > 0. && pdf_width > 0. {
            let ratio = pdf_width / natural;
            let (scale, spacing) = if (0.85..=1.2).contains(&ratio) {
                (100, (pdf_width - natural) / count)
            } else {
                let scale = (ratio * 100.).round().clamp(33., 300.);
                (scale as u32, (pdf_width - natural * scale / 100.) / count)
            };
            for run in &mut runs {
                run.scale = scale;
                run.spacing = twips(spacing).clamp(-1584, 1584);
            }
        }
        built.push(Segment {
            offset: (segment_x - start_x) * along,
            runs,
        });
    }
    if max_size <= 0. {
        return None;
    }
    let (ascent, descent) = extent;
    let line_height = (ascent + descent) * max_size;
    let last_segment = segments.last()?;
    let (_, last_char) = last_segment.1.last()?;
    let width = ((last_char.x + last_char.advance - start_x) * along).max(1.) * 1.05 + 2.;
    let height = line_height;
    // The baseline's start on the page, then the box's centre.
    let (bx, by) = (m[0] * start_x + m[4], m[1] * start_x + m[5]);
    let lift = (ascent - descent) / 2. * max_size;
    let (cx, cy) = (
        bx + cos * width / 2. - sin * lift,
        by + sin * width / 2. + cos * lift,
    );
    let _ = first;
    // Hidden when fills painted after it cover its start, middle and end.
    let text_length = (last_char.x + last_char.advance - start_x) * along;
    let raise = 0.3 * max_size;
    let samples = [0.05, 0.5, 0.95].map(|fraction| {
        let distance = fraction * text_length;
        (
            bx + cos * distance - sin * raise,
            by + sin * distance + cos * raise,
        )
    });
    let hidden = graphics
        .text_marks
        .iter()
        .filter(|mark| (mark.x - bx).hypot(mark.y - by) < 2.)
        .min_by(|a, b| {
            (a.x - bx)
                .hypot(a.y - by)
                .total_cmp(&(b.x - bx).hypot(b.y - by))
        })
        .is_some_and(|mark| graphics.covered_after(mark.order, &samples));
    let degrees = angle.to_degrees();
    let vertical = if (degrees - 90.).abs() < 2. {
        Some("vert270")
    } else if (degrees + 90.).abs() < 2. {
        Some("vert")
    } else {
        None
    };
    // An upright box for vertical writing: the line's box turned about its
    // centre.
    let (width, height) = match vertical {
        Some(_) => (height, width),
        None => (width, height),
    };
    let (left, top) = (
        cx - page_box[0] - width / 2.,
        page_box[3] - cy - height / 2.,
    );
    let rotation = (-degrees).rem_euclid(360.);
    Some(FixedLine {
        left,
        top,
        width,
        height,
        rotation: if vertical.is_some() || (rotation - 360.).abs() < 0.01 || rotation < 0.01 {
            0.
        } else {
            rotation
        },
        line_height,
        segments: built,
        hidden,
        vertical,
    })
}

fn same_style(a: &FixedRun, b: &FixedRun) -> bool {
    a.font == b.font
        && a.size == b.size
        && a.bold == b.bold
        && a.italic == b.italic
        && a.underline == b.underline
        && a.colour == b.colour
}

/// A run's style; its size is one that, scaled by `scale`, is a whole half
/// point, the sizes Word sets, so the width it is spaced to holds.
fn run_style(
    layer: &PageTextLayer,
    line: &PageTextLine,
    style: usize,
    up: f64,
    scale: f64,
) -> FixedRun {
    let style = &line.styles[style];
    let metrics = layer.font(&style.font);
    let (postscript, fallback) = match &style.face {
        Some(face) => (face.postscript_name.clone(), Some(face.fallback)),
        None => (
            metrics.and_then(|font| font.postscript_name.clone()),
            metrics.map(|font| font.fallback),
        ),
    };
    let family_class = fallback.map_or(FallbackFamily::Sans, |face| face.family);
    let (named, named_bold, named_italic) =
        crate::docx_export::font_family(postscript.as_deref().unwrap_or(""), family_class);
    let colour = style
        .fill
        .or(line.fill)
        .map(|rgb| rgb.map(|component| (component.clamp(0., 1.) * 255.).round() as u8));
    FixedRun {
        text: String::new(),
        font: word_family(&named, family_class),
        size: ((style.font_size.abs() * up * scale * 2.).round().max(1.) / 2.) / scale,
        bold: named_bold || fallback.is_some_and(|face| face.bold),
        italic: named_italic || fallback.is_some_and(|face| face.italic),
        underline: false,
        colour: colour.filter(|colour| *colour != [0, 0, 0]),
        spacing: 0,
        scale: 100,
    }
}

// --- WordprocessingML ------------------------------------------------------

/// What a fixed page's XML needs from the document: each SVG layer's and
/// each picture's relationship, and the fallback picture's.
pub(crate) struct FixedMedia<'a> {
    pub svg: &'a [String],
    pub pictures: &'a [PictureMedia],
    pub fallback: &'a str,
}

/// A picture's media: its relationship, and the opacity and crop its soft
/// mask stands for.
pub(crate) struct PictureMedia {
    pub relationship: String,
    pub alpha: f64,
    pub crop: Option<[f64; 4]>,
}

/// The body XML of a fixed page: one paragraph holding the page's pictures
/// and text boxes, ending its section unless it is the last page's.
/// Drawing ids start after `first_id`.
pub(crate) fn fixed_page_xml(
    page: &FixedPage,
    page_box: [f64; 4],
    media: &FixedMedia,
    first_id: usize,
    section: Option<&str>,
    body: &mut String,
) {
    let _ = write!(
        body,
        "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>{}\
         <w:rPr><w:sz w:val=\"2\"/></w:rPr></w:pPr>",
        section.unwrap_or("")
    );
    let mut id = first_id;
    let mut height = 1;
    // Objects in the order they stack, bottom first: hidden lines under
    // every picture. All lie in front of the (empty) page paragraph:
    // LibreOffice stacks objects behind the text in reverse.
    for line in page.lines.iter().filter(|line| line.hidden) {
        id += 1;
        height += 1;
        text_box_xml(body, id, height, line);
    }
    let (mut svg, mut picture) = (0, 0);
    let page_width = (page_box[2] - page_box[0]) * page.scale;
    let page_height = (page_box[3] - page_box[1]) * page.scale;
    for layer in &page.graphics.layers {
        id += 1;
        height += 1;
        match layer {
            GraphicsLayer::Svg(_) => {
                let relationship = &media.svg[svg];
                svg += 1;
                picture_xml(
                    body,
                    id,
                    height,
                    [0., 0., page_width, page_height],
                    None,
                    media.fallback,
                    Some(relationship),
                    (false, false),
                    1.,
                    0.,
                );
            }
            GraphicsLayer::Picture(image) => {
                let picture_media = &media.pictures[picture];
                picture += 1;
                // The mask's crop and the clip's, together.
                let mut image = image.clone();
                if let Some(mask) = picture_media.crop {
                    let clip = image.crop.unwrap_or([0.; 4]);
                    image.crop = Some([0, 1, 2, 3].map(|side| clip[side].max(mask[side])));
                }
                if image
                    .crop
                    .is_some_and(|[l, t, r, b]| l + r >= 1. || t + b >= 1.)
                {
                    continue;
                }
                let placed = placement(&image, page_box);
                picture_xml(
                    body,
                    id,
                    height,
                    placed.rect.map(|value| value * page.scale),
                    image.crop,
                    &picture_media.relationship,
                    None,
                    (placed.flip_horizontal, placed.flip_vertical),
                    picture_media.alpha,
                    placed.rotation,
                );
            }
        }
    }
    for line in page.lines.iter().filter(|line| !line.hidden) {
        id += 1;
        height += 1;
        text_box_xml(body, id, height, line);
    }
    body.push_str("</w:p>");
}

/// How a picture sits on the page: its box before turning (left, top,
/// width, height in points from the page's top left), its clockwise turn
/// in degrees, and whether it is mirrored across its box's width and
/// height, as Word applies them (mirror, then turn about the centre).
struct Placement {
    rect: [f64; 4],
    rotation: f64,
    flip_horizontal: bool,
    flip_vertical: bool,
}

fn placement(image: &PlacedImage, page_box: [f64; 4]) -> Placement {
    let m = image.matrix;
    let [l, t, r, b] = image.crop.unwrap_or([0.; 4]);
    // The shown part's extent along the image's own axes.
    let width = m[0].hypot(m[1]) * (1. - l - r);
    let height = m[2].hypot(m[3]) * (1. - t - b);
    let corners = image.shown_corners();
    let centre = (
        corners.iter().map(|(x, _)| x).sum::<f64>() / 4.,
        corners.iter().map(|(_, y)| y).sum::<f64>() / 4.,
    );
    // On the page, y downward: the image's x axis, and its downward axis
    // (from its first row to its last).
    let across = (m[0], -m[1]);
    let down = (-m[2], m[3]);
    let turn = across.1.atan2(across.0);
    // Unmirrored, the box's downward axis turns to this.
    let box_down = (-turn.sin(), turn.cos());
    let flip_vertical = box_down.0 * down.0 + box_down.1 * down.1 < 0.;
    let mut rotation = turn.to_degrees().rem_euclid(360.);
    if (rotation - 360.).abs() < 0.01 || rotation < 0.01 {
        rotation = 0.;
    }
    let (cx, cy) = (centre.0 - page_box[0], page_box[3] - centre.1);
    Placement {
        rect: [cx - width / 2., cy - height / 2., width, height],
        rotation,
        flip_horizontal: false,
        flip_vertical,
    }
}

fn anchor_open(
    body: &mut String,
    id: usize,
    height: usize,
    behind: bool,
    left: f64,
    top: f64,
    width: f64,
    height_points: f64,
) {
    let _ = write!(
        body,
        "<w:r><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" simplePos=\"0\" \
         relativeHeight=\"{height}\" behindDoc=\"{}\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\">\
         <wp:simplePos x=\"0\" y=\"0\"/>\
         <wp:positionH relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionH>\
         <wp:positionV relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionV>\
         <wp:extent cx=\"{}\" cy=\"{}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:wrapNone/>",
        u8::from(behind),
        emu(left),
        emu(top),
        emu(width.max(0.01)),
        emu(height_points.max(0.01)),
    );
    let _ = id;
}

fn picture_xml(
    body: &mut String,
    id: usize,
    height: usize,
    [left, top, width, height_points]: [f64; 4],
    crop: Option<[f64; 4]>,
    relationship: &str,
    svg: Option<&str>,
    (flip_horizontal, flip_vertical): (bool, bool),
    alpha: f64,
    rotation: f64,
) {
    anchor_open(body, id, height, false, left, top, width, height_points);
    let flips = format!(
        "{}{}{}",
        if rotation == 0. {
            String::new()
        } else {
            format!(" rot=\"{}\"", (rotation * 60_000.).round() as i64)
        },
        if flip_horizontal { " flipH=\"1\"" } else { "" },
        if flip_vertical { " flipV=\"1\"" } else { "" }
    );
    let (cx, cy) = (emu(width.max(0.01)), emu(height_points.max(0.01)));
    let _ = write!(
        body,
        "<wp:docPr id=\"{id}\" name=\"Picture {id}\"/><wp:cNvGraphicFramePr/>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"Picture {id}\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"{relationship}\">"
    );
    if alpha < 1. {
        let _ = write!(
            body,
            "<a:alphaModFix amt=\"{}\"/>",
            (alpha * 100_000.).round() as i64
        );
    }
    if let Some(svg) = svg {
        let _ = write!(
            body,
            "<a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\">\
             <asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{svg}\"/>\
             </a:ext></a:extLst>"
        );
    }
    body.push_str("</a:blip>");
    if let Some([l, t, r, b]) = crop {
        let per_mille = |fraction: f64| (fraction * 100_000.).round() as i64;
        let _ = write!(
            body,
            "<a:srcRect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>",
            per_mille(l),
            per_mille(t),
            per_mille(r),
            per_mille(b)
        );
    }
    let _ = write!(
        body,
        "<a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm{flips}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
         </a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"
    );
}

fn text_box_xml(body: &mut String, id: usize, height: usize, line: &FixedLine) {
    anchor_open(
        body,
        id,
        height,
        false,
        line.left,
        line.top,
        line.width,
        line.height,
    );
    let (cx, cy) = (emu(line.width.max(0.01)), emu(line.height.max(0.01)));
    let rotation = if line.rotation == 0. {
        String::new()
    } else {
        format!(" rot=\"{}\"", (line.rotation * 60_000.).round() as i64)
    };
    let _ = write!(
        body,
        "<wp:docPr id=\"{id}\" name=\"Text {id}\"/><wp:cNvGraphicFramePr/>\
         <a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">\
         <wps:wsp><wps:cNvSpPr txBox=\"1\"/><wps:spPr><a:xfrm{rotation}><a:off x=\"0\" y=\"0\"/>\
         <a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>\
         <a:noFill/><a:ln><a:noFill/></a:ln></wps:spPr><wps:txbx><w:txbxContent><w:p><w:pPr>"
    );
    if line.segments.len() > 1 {
        body.push_str("<w:tabs>");
        for segment in &line.segments[1..] {
            let _ = write!(
                body,
                "<w:tab w:val=\"left\" w:pos=\"{}\"/>",
                twips(segment.offset)
            );
        }
        body.push_str("</w:tabs>");
    }
    let _ = write!(
        body,
        "<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"{}\" w:lineRule=\"exact\"/>\
         <w:ind w:left=\"0\" w:right=\"0\" w:firstLine=\"0\"/><w:jc w:val=\"left\"/></w:pPr>",
        twips(line.line_height).max(1)
    );
    for (index, segment) in line.segments.iter().enumerate() {
        if index > 0 {
            body.push_str("<w:r><w:tab/></w:r>");
        }
        for run in &segment.runs {
            run_xml(body, run);
        }
    }
    body.push_str(
        "</w:p></w:txbxContent></wps:txbx>\
         <wps:bodyPr rot=\"0\" vert=\"",
    );
    body.push_str(line.vertical.unwrap_or("horz"));
    body.push_str(
        "\" wrap=\"none\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" \
         anchor=\"t\" anchorCtr=\"0\" upright=\"0\"><a:noAutofit/></wps:bodyPr></wps:wsp>\
         </a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>",
    );
}

fn run_xml(body: &mut String, run: &FixedRun) {
    let font = crate::docx_export::escape(&run.font);
    let half_points = (run.size * 2.).round().max(1.) as i64;
    let _ = write!(
        body,
        "<w:r><w:rPr><w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:cs=\"{font}\" w:eastAsia=\"{font}\"/>"
    );
    if run.bold {
        body.push_str("<w:b/>");
    }
    if run.italic {
        body.push_str("<w:i/>");
    }
    if let Some([r, g, b]) = run.colour {
        let _ = write!(body, "<w:color w:val=\"{r:02X}{g:02X}{b:02X}\"/>");
    }
    if run.spacing != 0 {
        let _ = write!(body, "<w:spacing w:val=\"{}\"/>", run.spacing);
    }
    if run.scale != 100 {
        let _ = write!(body, "<w:w w:val=\"{}\"/>", run.scale);
    }
    // Half-points carry the size; a size between them is kept by `w:kern`'s
    // neighbour, `w:sz`, rounding to the nearest half point.
    let _ = write!(
        body,
        "<w:sz w:val=\"{half_points}\"/><w:szCs w:val=\"{half_points}\"/>"
    );
    if run.underline {
        body.push_str("<w:u w:val=\"single\"/>");
    }
    body.push_str("</w:rPr><w:t xml:space=\"preserve\">");
    crate::docx_export::escape_into(&run.text, body);
    body.push_str("</w:t></w:r>");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncommon_fonts_are_drawn_in_the_common_family_of_their_kind() {
        assert_eq!(word_family("Swiss721BT", FallbackFamily::Sans), "Arial");
        assert_eq!(word_family("Helvetica", FallbackFamily::Sans), "Helvetica");
        assert_eq!(word_family("Calibri", FallbackFamily::Sans), "Calibri");
        assert_eq!(
            word_family("Minion Pro", FallbackFamily::Serif),
            "Times New Roman"
        );
        assert_eq!(
            word_family("Nimbus Mono PS", FallbackFamily::Sans),
            "Courier New"
        );
    }

    #[test]
    fn arial_metrics_measure_text() {
        let face = face(Metrics::Sans, false, false);
        // Arial's "M" is 0.833 em; its space 0.278 em.
        assert!((face.advance('M') - 0.833).abs() < 0.002);
        assert!((face.advance(' ') - 0.278).abs() < 0.002);
        let (ascent, descent) = face.extent();
        assert!((ascent - 0.905).abs() < 0.01 && (descent - 0.212).abs() < 0.01);
    }
}
