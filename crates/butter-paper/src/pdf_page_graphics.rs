//! A page's drawings and images, for a Word document page that keeps the
//! PDF's layout (see `docx_export`): what the page's content paints apart
//! from its text, in paint order, as layers a word processor draws behind
//! the text. Paths become SVG (crisp at any zoom, as Word 2016 and later
//! and LibreOffice draw it); an image drawn upright, clipped to at most a
//! rectangle, is a picture of its own (a JPEG stays as it is), and any
//! other image goes into the SVG.
//!
//! The reader covers the PDF imaging model a drawing set uses: paths,
//! their fill rule, stroke width, caps, joins, miter limit and dashes,
//! clipping, device, ICC and indexed colours, opacity, images (soft masks
//! and stencil masks included) and Form XObjects. Text is left to the
//! caller, as the Word document holds it as text. Shadings, patterns, soft
//! mask groups and blend modes are not drawn.

use std::collections::HashSet;
use std::fmt::Write as _;

use base64::Engine as _;
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

/// How deeply Form XObjects may nest, and the most operators a page may
/// paint before the rest is left out.
const MAX_FORM_DEPTH: usize = 12;
const MAX_OPERATIONS: usize = 5_000_000;
/// The narrowest a stroke is drawn, in points: a PDF's zero-width line is
/// the thinnest the device can draw, which SVG would not draw at all.
const THINNEST_STROKE: f64 = 0.24;

type Matrix = [f64; 6];
const IDENTITY: Matrix = [1., 0., 0., 1., 0., 0.];

fn multiply(left: Matrix, right: Matrix) -> Matrix {
    [
        left[0] * right[0] + left[1] * right[2],
        left[0] * right[1] + left[1] * right[3],
        left[2] * right[0] + left[3] * right[2],
        left[2] * right[1] + left[3] * right[3],
        left[4] * right[0] + left[5] * right[2] + right[4],
        left[4] * right[1] + left[5] * right[3] + right[5],
    ]
}

fn apply(m: Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// What a page draws apart from its text, in paint order.
#[derive(Debug, Default)]
pub struct PageGraphics {
    pub layers: Vec<GraphicsLayer>,
    /// Paths painted that are more than a straight line or a rectangle.
    pub shapes: usize,
    /// Images drawn into the SVG (at an angle, flipped, or clipped to a
    /// shape).
    pub embedded_images: usize,
    /// Where each text-showing operator starts drawing, in page space, and
    /// when, in paint order.
    pub text_marks: Vec<TextMark>,
    /// Opaque fills, in paint order: what covers text drawn before them.
    pub covers: Vec<Cover>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMark {
    pub x: f64,
    pub y: f64,
    pub order: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cover {
    pub order: usize,
    /// Left, bottom, right, top in page space.
    pub bounds: [f64; 4],
    /// Each subpath's points in page space (curves by their control
    /// points).
    pub subpaths: Vec<Vec<(f64, f64)>>,
    pub even_odd: bool,
}

impl Cover {
    /// Whether the fill covers page point (`x`, `y`).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let [l, b, r, t] = self.bounds;
        if x < l || x > r || y < b || y > t {
            return false;
        }
        let mut winding = 0i32;
        let mut crossings = 0u32;
        for subpath in &self.subpaths {
            let count = subpath.len();
            if count < 3 {
                continue;
            }
            for index in 0..count {
                let (x0, y0) = subpath[index];
                let (x1, y1) = subpath[(index + 1) % count];
                if (y0 <= y) != (y1 <= y) {
                    let at = x0 + (y - y0) / (y1 - y0) * (x1 - x0);
                    if at > x {
                        crossings += 1;
                        winding += if y1 > y0 { 1 } else { -1 };
                    }
                }
            }
        }
        if self.even_odd {
            crossings % 2 == 1
        } else {
            winding != 0
        }
    }
}

impl PageGraphics {
    /// Whether text drawn at `order` is hidden at every one of `points` by
    /// an opaque fill painted after it.
    pub fn covered_after(&self, order: usize, points: &[(f64, f64)]) -> bool {
        let start = self.covers.partition_point(|cover| cover.order <= order);
        points.iter().all(|(x, y)| {
            self.covers[start..]
                .iter()
                .any(|cover| cover.contains(*x, *y))
        })
    }
}

/// Text state enough to find where text-showing operators start.
#[derive(Clone, Copy, Debug)]
struct TextPosition {
    matrix: Matrix,
    line: Matrix,
    leading: f64,
}

impl Default for TextPosition {
    fn default() -> Self {
        Self {
            matrix: IDENTITY,
            line: IDENTITY,
            leading: 0.,
        }
    }
}

impl TextPosition {
    fn move_line(&mut self, tx: f64, ty: f64) {
        self.line = multiply([1., 0., 0., 1., tx, ty], self.line);
        self.matrix = self.line;
    }
}

#[derive(Debug)]
pub enum GraphicsLayer {
    /// An SVG document the page's size, its user units PDF points, drawn
    /// over the page's box.
    Svg(String),
    /// An image drawn upright, as its own picture.
    Picture(PlacedImage),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlacedImage {
    pub object: ObjectId,
    /// Maps the image's unit square to page space, as the PDF draws it.
    pub matrix: [f64; 6],
    /// The part of the image shown, as fractions of it cut from each side
    /// (left, top, right, bottom, of the image as stored), when a clip cuts
    /// it.
    pub crop: Option<[f64; 4]>,
    /// Nothing is drawn before it: only the paper lies beneath.
    pub over_paper: bool,
}

impl PlacedImage {
    /// Where the shown part of the image lies on the page, its corners in
    /// page space.
    pub fn shown_corners(&self) -> [(f64, f64); 4] {
        let [l, t, r, b] = self.crop.unwrap_or([0.; 4]);
        // Image space: u across, v up from the bottom row.
        [(l, b), (1. - r, b), (1. - r, 1. - t), (l, 1. - t)].map(|(u, v)| apply(self.matrix, u, v))
    }
}

/// An image the caller encodes (JPEG as it is, anything else PNG), by its
/// stream's object; `None` when it cannot be drawn.
pub type ImageEncoder<'a> = dyn Fn(ObjectId) -> Option<(Vec<u8>, &'static str)> + Sync + 'a;

/// What page `page_id` draws apart from its text, over its visible box
/// `page_box` (left, bottom, right, top).
pub fn page_graphics(
    document: &Document,
    page_id: ObjectId,
    page_box: [f64; 4],
    encode: &ImageEncoder,
) -> PageGraphics {
    let resources = crate::pdf_text_edit::page_resources(document, page_id).unwrap_or_default();
    let bytes = document.get_page_content(page_id);
    let Ok(content) = Content::decode(&bytes) else {
        return PageGraphics::default();
    };
    let mut painter = Painter {
        document,
        encode,
        page_box,
        layers: Vec::new(),
        svg: SvgLayer::default(),
        operations: 0,
        forms: HashSet::new(),
        shapes: 0,
        embedded_images: 0,
        text_marks: Vec::new(),
        covers: Vec::new(),
        order: 0,
    };
    let state = GraphicsState::default();
    painter.run(&content.operations, &resources, state, 0);
    painter.annotations(page_id);
    painter.finish_layer();
    PageGraphics {
        layers: painter.layers,
        shapes: painter.shapes,
        embedded_images: painter.embedded_images,
        text_marks: painter.text_marks,
        covers: painter.covers,
    }
}

// --- Graphics state ------------------------------------------------------

#[derive(Clone, Debug)]
struct Clip {
    /// SVG path data in page space.
    path: String,
    even_odd: bool,
    /// The clip's bounds in page space, when it is an axis-aligned
    /// rectangle.
    rect: Option<[f64; 4]>,
}

#[derive(Clone, Debug)]
struct GraphicsState {
    ctm: Matrix,
    fill: Option<[u8; 3]>,
    stroke: Option<[u8; 3]>,
    fill_space: ColourSpace,
    stroke_space: ColourSpace,
    fill_alpha: f64,
    stroke_alpha: f64,
    line_width: f64,
    cap: i64,
    join: i64,
    miter: f64,
    dash: Vec<f64>,
    dash_phase: f64,
    clips: Vec<Clip>,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: IDENTITY,
            fill: Some([0, 0, 0]),
            stroke: Some([0, 0, 0]),
            fill_space: ColourSpace::Gray,
            stroke_space: ColourSpace::Gray,
            fill_alpha: 1.,
            stroke_alpha: 1.,
            line_width: 1.,
            cap: 0,
            join: 0,
            miter: 10.,
            dash: Vec::new(),
            dash_phase: 0.,
            clips: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
enum ColourSpace {
    Gray,
    Rgb,
    Cmyk,
    /// A palette of RGB colours.
    Indexed(Vec<[u8; 3]>),
    /// A Separation or DeviceN space, drawn as a grey of its tint.
    Tint,
    /// A pattern: what it paints is not drawn.
    Pattern,
}

impl ColourSpace {
    fn colour(&self, values: &[f64]) -> Option<[u8; 3]> {
        let byte = |value: f64| (value.clamp(0., 1.) * 255.).round() as u8;
        match self {
            ColourSpace::Gray => values.first().map(|gray| [byte(*gray); 3]),
            ColourSpace::Rgb => {
                (values.len() >= 3).then(|| [byte(values[0]), byte(values[1]), byte(values[2])])
            }
            ColourSpace::Cmyk => (values.len() >= 4).then(|| {
                let k = values[3];
                [
                    byte((1. - values[0]) * (1. - k)),
                    byte((1. - values[1]) * (1. - k)),
                    byte((1. - values[2]) * (1. - k)),
                ]
            }),
            ColourSpace::Indexed(palette) => values
                .first()
                .and_then(|index| palette.get(index.max(0.) as usize).copied()),
            ColourSpace::Tint => {
                let tint = values.iter().copied().fold(0., f64::max);
                Some([byte(1. - tint); 3])
            }
            ColourSpace::Pattern => None,
        }
    }
}

// --- SVG ---------------------------------------------------------------

#[derive(Default)]
struct SvgLayer {
    defs: String,
    body: String,
    /// The clip chain each emitted clipPath stands for, and its id.
    clip_ids: Vec<(Vec<String>, usize)>,
    next_id: usize,
    /// The clip id of the open `<g>`, if one is open.
    open_group: Option<Option<usize>>,
    painted: bool,
}

impl SvgLayer {
    /// The clipPath id for `clips`, defining it (and its outer clips) if new.
    fn clip_id(&mut self, clips: &[Clip]) -> Option<usize> {
        if clips.is_empty() {
            return None;
        }
        let keys = clips.iter().map(clip_key).collect::<Vec<_>>();
        if let Some((_, id)) = self.clip_ids.iter().find(|(chain, _)| *chain == keys) {
            return Some(*id);
        }
        let outer = self.clip_id(&clips[..clips.len() - 1]);
        let id = self.next_id;
        self.next_id += 1;
        let clip = &clips[clips.len() - 1];
        let _ = write!(self.defs, "<clipPath id=\"c{id}\"");
        if let Some(outer) = outer {
            let _ = write!(self.defs, " clip-path=\"url(#c{outer})\"");
        }
        let _ = write!(
            self.defs,
            "><path d=\"{}\"{}/></clipPath>",
            clip.path,
            if clip.even_odd {
                " clip-rule=\"evenodd\""
            } else {
                ""
            }
        );
        self.clip_ids.push((keys, id));
        Some(id)
    }

    /// Opens a group clipped to `clips` for what is painted next.
    fn group(&mut self, clips: &[Clip]) {
        let id = self.clip_id(clips);
        if self.open_group == Some(id) {
            return;
        }
        if self.open_group.is_some() {
            self.body.push_str("</g>");
        }
        match id {
            Some(id) => {
                let _ = write!(self.body, "<g clip-path=\"url(#c{id})\">");
            }
            None => self.body.push_str("<g>"),
        }
        self.open_group = Some(id);
        self.painted = true;
    }

    fn document(mut self, page_box: [f64; 4]) -> String {
        if self.open_group.is_some() {
            self.body.push_str("</g>");
        }
        let [left, bottom, right, top] = page_box;
        let (width, height) = (right - left, top - bottom);
        let mut svg = String::with_capacity(self.defs.len() + self.body.len() + 400);
        let _ = write!(
            svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {} {}\">",
            num(width),
            num(height),
            num(width),
            num(height)
        );
        if !self.defs.is_empty() {
            svg.push_str("<defs>");
            svg.push_str(&self.defs);
            svg.push_str("</defs>");
        }
        // PDF space, y up, into SVG's, y down.
        let _ = write!(
            svg,
            "<g transform=\"matrix(1 0 0 -1 {} {})\">",
            num(-left),
            num(top)
        );
        svg.push_str(&self.body);
        svg.push_str("</g></svg>");
        svg
    }
}

/// An SVG layer of [`page_graphics`] shown at `size` (width, height in
/// points): its `width` and `height` changed, its drawing (its viewBox)
/// unchanged.
pub fn with_display_size(svg: String, (width, height): (f64, f64)) -> String {
    let Some(end) = svg.find(" viewBox=") else {
        return svg;
    };
    let Some(start) = svg[..end].find(" width=") else {
        return svg;
    };
    format!(
        "{} width=\"{}pt\" height=\"{}pt\"{}",
        &svg[..start],
        num(width),
        num(height),
        &svg[end..]
    )
}

fn clip_key(clip: &Clip) -> String {
    format!("{}{}", if clip.even_odd { "*" } else { "" }, clip.path)
}

/// A number for SVG: at most three decimals, without trailing zeros.
fn num(value: f64) -> String {
    let mut text = String::new();
    push_num(&mut text, value);
    text
}

fn push_num(out: &mut String, value: f64) {
    if !value.is_finite() {
        out.push('0');
        return;
    }
    let rounded = (value * 1000.).round() / 1000.;
    if rounded == rounded.trunc() && rounded.abs() < 1e15 {
        let _ = write!(out, "{}", rounded as i64);
        return;
    }
    let start = out.len();
    let _ = write!(out, "{rounded:.3}");
    while out.ends_with('0') {
        out.pop();
    }
    if out.ends_with('.') {
        out.pop();
    }
    if out.len() == start {
        out.push('0');
    }
}

fn hex(colour: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
}

// --- Painting --------------------------------------------------------------

struct Painter<'a> {
    document: &'a Document,
    encode: &'a ImageEncoder<'a>,
    page_box: [f64; 4],
    layers: Vec<GraphicsLayer>,
    svg: SvgLayer,
    operations: usize,
    /// Form XObjects being drawn, so one drawing itself is not followed.
    forms: HashSet<ObjectId>,
    shapes: usize,
    embedded_images: usize,
    text_marks: Vec<TextMark>,
    covers: Vec<Cover>,
    /// Paint order: each painting or text-showing operator's number.
    order: usize,
}

/// The path being built, in user space, and in page space for clipping.
#[derive(Default)]
struct PathBuilder {
    user: String,
    page: String,
    current: Option<(f64, f64)>,
    /// Page-space points, for a rectangle test.
    points: Vec<(f64, f64)>,
    rectangle: bool,
    segments: usize,
    /// A curve or more than one subpath: not a rectangle.
    curved: bool,
    subpaths: usize,
    /// Each subpath's page-space points.
    outline: Vec<Vec<(f64, f64)>>,
}

impl PathBuilder {
    /// The path's bounds when it is one axis-aligned rectangle: `re`, or
    /// four straight sides (closed by `h` or a fifth point on the first).
    fn rect(&self) -> Option<[f64; 4]> {
        if self.rectangle {
            return rect_of(&self.points);
        }
        if self.curved || self.subpaths != 1 {
            return None;
        }
        let mut points = self.points.clone();
        if points.len() == 5 {
            let (first, last) = (points[0], points[4]);
            if (first.0 - last.0).abs() < 0.01 && (first.1 - last.1).abs() < 0.01 {
                points.pop();
            }
        }
        if points.len() != 4 {
            return None;
        }
        // Consecutive corners share an x or a y.
        let sides = (0..4).all(|index| {
            let (a, b) = (points[index], points[(index + 1) % 4]);
            (a.0 - b.0).abs() < 0.01 || (a.1 - b.1).abs() < 0.01
        });
        sides.then(|| rect_of(&points)).flatten()
    }

    fn clear(&mut self) {
        *self = PathBuilder::default();
    }

    fn push(&mut self, ctm: Matrix, command: char, coordinates: &[f64]) {
        self.user.push(command);
        self.page.push(command);
        if command == 'M' || self.outline.is_empty() {
            self.outline.push(Vec::new());
        }
        for pair in coordinates.chunks(2) {
            let (x, y) = (pair[0], pair[1]);
            push_num(&mut self.user, x);
            self.user.push(' ');
            push_num(&mut self.user, y);
            self.user.push(' ');
            let (px, py) = apply(ctm, x, y);
            push_num(&mut self.page, px);
            self.page.push(' ');
            push_num(&mut self.page, py);
            self.page.push(' ');
            self.points.push((px, py));
            if let Some(subpath) = self.outline.last_mut() {
                subpath.push((px, py));
            }
        }
        self.segments += 1;
    }
}

fn numbers(operands: &[Object]) -> Vec<f64> {
    operands
        .iter()
        .filter_map(|operand| match operand {
            Object::Integer(value) => Some(*value as f64),
            Object::Real(value) => Some(f64::from(*value)),
            _ => None,
        })
        .collect()
}

impl<'a> Painter<'a> {
    fn resolve<'o>(&'o self, object: &'o Object) -> Option<&'o Object> {
        self.document
            .dereference(object)
            .ok()
            .map(|(_, object)| object)
    }

    fn dict<'o>(&'o self, object: &'o Object) -> Option<&'o Dictionary> {
        match self.resolve(object)? {
            Object::Dictionary(dict) => Some(dict),
            Object::Stream(stream) => Some(&stream.dict),
            _ => None,
        }
    }

    fn resource<'o>(
        &'o self,
        resources: &'o Dictionary,
        kind: &[u8],
        name: &[u8],
    ) -> Option<&'o Object> {
        let category = self.dict(resources.get(kind).ok()?)?;
        category.get(name).ok()
    }

    fn colour_space(&self, resources: &Dictionary, operand: &Object) -> ColourSpace {
        let space = match operand {
            Object::Name(name) => match name.as_slice() {
                b"DeviceGray" | b"G" | b"CalGray" => return ColourSpace::Gray,
                b"DeviceRGB" | b"RGB" | b"CalRGB" => return ColourSpace::Rgb,
                b"DeviceCMYK" | b"CMYK" => return ColourSpace::Cmyk,
                b"Pattern" => return ColourSpace::Pattern,
                _ => match self.resource(resources, b"ColorSpace", name) {
                    Some(space) => space,
                    None => return ColourSpace::Gray,
                },
            },
            other => other,
        };
        self.colour_space_object(space, 0)
    }

    fn colour_space_object(&self, space: &Object, depth: usize) -> ColourSpace {
        if depth > 4 {
            return ColourSpace::Gray;
        }
        let Some(space) = self.resolve(space) else {
            return ColourSpace::Gray;
        };
        match space {
            Object::Name(name) => match name.as_slice() {
                b"DeviceRGB" | b"CalRGB" => ColourSpace::Rgb,
                b"DeviceCMYK" => ColourSpace::Cmyk,
                b"Pattern" => ColourSpace::Pattern,
                _ => ColourSpace::Gray,
            },
            Object::Array(items) => {
                let family = items.first().and_then(|name| name.as_name().ok());
                match family {
                    Some(b"ICCBased") => {
                        let components = items
                            .get(1)
                            .and_then(|stream| self.dict(stream))
                            .and_then(|dict| dict.get(b"N").ok())
                            .and_then(|n| n.as_i64().ok())
                            .unwrap_or(3);
                        match components {
                            1 => ColourSpace::Gray,
                            4 => ColourSpace::Cmyk,
                            _ => ColourSpace::Rgb,
                        }
                    }
                    Some(b"CalRGB" | b"Lab") => ColourSpace::Rgb,
                    Some(b"CalGray") => ColourSpace::Gray,
                    Some(b"Indexed" | b"I") => {
                        let base = items
                            .get(1)
                            .map(|base| self.colour_space_object(base, depth + 1))
                            .unwrap_or(ColourSpace::Rgb);
                        let lookup =
                            items.get(3).and_then(|lookup| match self.resolve(lookup)? {
                                Object::String(bytes, _) => Some(bytes.clone()),
                                Object::Stream(stream) => stream
                                    .decompressed_content()
                                    .ok()
                                    .or_else(|| Some(stream.content.clone())),
                                _ => None,
                            });
                        let width = match base {
                            ColourSpace::Gray => 1,
                            ColourSpace::Cmyk => 4,
                            _ => 3,
                        };
                        let palette = lookup
                            .unwrap_or_default()
                            .chunks(width)
                            .filter(|entry| entry.len() == width)
                            .map(|entry| {
                                let values = entry
                                    .iter()
                                    .map(|byte| f64::from(*byte) / 255.)
                                    .collect::<Vec<_>>();
                                base.colour(&values).unwrap_or([0; 3])
                            })
                            .collect();
                        ColourSpace::Indexed(palette)
                    }
                    Some(b"Separation" | b"DeviceN") => ColourSpace::Tint,
                    Some(b"Pattern") => ColourSpace::Pattern,
                    _ => ColourSpace::Gray,
                }
            }
            _ => ColourSpace::Gray,
        }
    }

    fn run(
        &mut self,
        operations: &[lopdf::content::Operation],
        resources: &Dictionary,
        mut state: GraphicsState,
        depth: usize,
    ) {
        let mut stack: Vec<GraphicsState> = Vec::new();
        let mut path = PathBuilder::default();
        let mut pending_clip: Option<bool> = None;
        let mut in_text = false;
        let mut text = TextPosition::default();
        for operation in operations {
            self.operations += 1;
            if self.operations > MAX_OPERATIONS {
                return;
            }
            let operands = &operation.operands;
            let n = || numbers(operands);
            match operation.operator.as_str() {
                "q" => stack.push(state.clone()),
                "Q" => {
                    if let Some(saved) = stack.pop() {
                        state = saved;
                    }
                }
                "cm" => {
                    let v = n();
                    if v.len() == 6 {
                        state.ctm = multiply([v[0], v[1], v[2], v[3], v[4], v[5]], state.ctm);
                    }
                }
                "w" => {
                    if let Some(width) = n().first() {
                        state.line_width = *width;
                    }
                }
                "J" => {
                    if let Some(cap) = n().first() {
                        state.cap = *cap as i64;
                    }
                }
                "j" => {
                    if let Some(join) = n().first() {
                        state.join = *join as i64;
                    }
                }
                "M" => {
                    if let Some(miter) = n().first() {
                        state.miter = *miter;
                    }
                }
                "d" => {
                    if let (Some(Object::Array(array)), Some(phase)) =
                        (operands.first(), operands.get(1))
                    {
                        state.dash = numbers(array);
                        state.dash_phase = numbers(std::slice::from_ref(phase))
                            .first()
                            .copied()
                            .unwrap_or(0.);
                    }
                }
                "gs" => {
                    if let Some(Object::Name(name)) = operands.first()
                        && let Some(dict) = self
                            .resource(resources, b"ExtGState", name)
                            .and_then(|state| self.dict(state))
                    {
                        self.ext_gstate(dict, &mut state);
                    }
                }
                "g" => {
                    state.fill_space = ColourSpace::Gray;
                    state.fill = ColourSpace::Gray.colour(&n());
                }
                "G" => {
                    state.stroke_space = ColourSpace::Gray;
                    state.stroke = ColourSpace::Gray.colour(&n());
                }
                "rg" => {
                    state.fill_space = ColourSpace::Rgb;
                    state.fill = ColourSpace::Rgb.colour(&n());
                }
                "RG" => {
                    state.stroke_space = ColourSpace::Rgb;
                    state.stroke = ColourSpace::Rgb.colour(&n());
                }
                "k" => {
                    state.fill_space = ColourSpace::Cmyk;
                    state.fill = ColourSpace::Cmyk.colour(&n());
                }
                "K" => {
                    state.stroke_space = ColourSpace::Cmyk;
                    state.stroke = ColourSpace::Cmyk.colour(&n());
                }
                "cs" => {
                    if let Some(space) = operands.first() {
                        state.fill_space = self.colour_space(resources, space);
                        state.fill = match &state.fill_space {
                            ColourSpace::Cmyk => Some([0, 0, 0]),
                            ColourSpace::Tint => Some([0, 0, 0]),
                            space => space.colour(&[0., 0., 0.]),
                        };
                    }
                }
                "CS" => {
                    if let Some(space) = operands.first() {
                        state.stroke_space = self.colour_space(resources, space);
                        state.stroke = match &state.stroke_space {
                            ColourSpace::Cmyk => Some([0, 0, 0]),
                            ColourSpace::Tint => Some([0, 0, 0]),
                            space => space.colour(&[0., 0., 0.]),
                        };
                    }
                }
                "sc" | "scn" => state.fill = state.fill_space.colour(&n()),
                "SC" | "SCN" => state.stroke = state.stroke_space.colour(&n()),
                "BT" => {
                    in_text = true;
                    text = TextPosition::default();
                }
                "ET" => in_text = false,
                "Tm" => {
                    let v = n();
                    if v.len() == 6 {
                        text.line = [v[0], v[1], v[2], v[3], v[4], v[5]];
                        text.matrix = text.line;
                    }
                }
                "Td" => {
                    let v = n();
                    if v.len() >= 2 {
                        text.move_line(v[0], v[1]);
                    }
                }
                "TD" => {
                    let v = n();
                    if v.len() >= 2 {
                        text.leading = -v[1];
                        text.move_line(v[0], v[1]);
                    }
                }
                "TL" => {
                    if let Some(leading) = n().first() {
                        text.leading = *leading;
                    }
                }
                "T*" => text.move_line(0., -text.leading),
                "Tj" | "TJ" | "'" | "\"" => {
                    if matches!(operation.operator.as_str(), "'" | "\"") {
                        text.move_line(0., -text.leading);
                    }
                    self.order += 1;
                    let (x, y) = apply(multiply(text.matrix, state.ctm), 0., 0.);
                    self.text_marks.push(TextMark {
                        x,
                        y,
                        order: self.order,
                    });
                }
                "m" | "l" if !in_text => {
                    let v = n();
                    if v.len() >= 2 {
                        let command = if operation.operator == "m" { 'M' } else { 'L' };
                        if command == 'M' {
                            path.subpaths += 1;
                        }
                        path.push(state.ctm, command, &v[..2]);
                        path.current = Some((v[0], v[1]));
                    }
                }
                "c" if !in_text => {
                    path.curved = true;
                    let v = n();
                    if v.len() >= 6 {
                        path.push(state.ctm, 'C', &v[..6]);
                        path.current = Some((v[4], v[5]));
                    }
                }
                "v" if !in_text => {
                    path.curved = true;
                    let v = n();
                    if v.len() >= 4
                        && let Some((x, y)) = path.current
                    {
                        path.push(state.ctm, 'C', &[x, y, v[0], v[1], v[2], v[3]]);
                        path.current = Some((v[2], v[3]));
                    }
                }
                "y" if !in_text => {
                    path.curved = true;
                    let v = n();
                    if v.len() >= 4 {
                        path.push(state.ctm, 'C', &[v[0], v[1], v[2], v[3], v[2], v[3]]);
                        path.current = Some((v[2], v[3]));
                    }
                }
                "h" if !in_text => {
                    path.user.push('Z');
                    path.page.push('Z');
                }
                "re" if !in_text => {
                    let v = n();
                    if v.len() >= 4 {
                        let (x, y, w, h) = (v[0], v[1], v[2], v[3]);
                        let lone = path.segments == 0;
                        path.subpaths += 1;
                        path.push(state.ctm, 'M', &[x, y]);
                        path.push(state.ctm, 'L', &[x + w, y]);
                        path.push(state.ctm, 'L', &[x + w, y + h]);
                        path.push(state.ctm, 'L', &[x, y + h]);
                        path.user.push('Z');
                        path.page.push('Z');
                        path.current = Some((x, y));
                        path.rectangle = lone;
                    }
                }
                "W" => pending_clip = Some(false),
                "W*" => pending_clip = Some(true),
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                    let operator = operation.operator.as_str();
                    if matches!(operator, "s" | "b" | "b*") {
                        path.user.push('Z');
                        path.page.push('Z');
                    }
                    let fill = matches!(operator, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
                    let stroke = matches!(operator, "S" | "s" | "B" | "B*" | "b" | "b*");
                    let even_odd = matches!(operator, "f*" | "B*" | "b*");
                    if (fill || stroke) && path.segments > 0 {
                        self.paint_path(&path, &state, fill, stroke, even_odd);
                    }
                    if let Some(even_odd) = pending_clip.take()
                        && path.segments > 0
                    {
                        let rect = path.rect();
                        state.clips.push(Clip {
                            path: path.page.clone(),
                            even_odd,
                            rect,
                        });
                    }
                    path.clear();
                }
                "Do" => {
                    if let Some(Object::Name(name)) = operands.first() {
                        self.draw_xobject(resources, name, &state, depth);
                    }
                }
                "BI" => {
                    if let Some(Object::Stream(stream)) = operands.first() {
                        self.inline_image(stream, resources, &state);
                    }
                }
                _ => {}
            }
        }
    }

    fn ext_gstate(&self, dict: &Dictionary, state: &mut GraphicsState) {
        let number = |key: &[u8]| {
            dict.get(key)
                .ok()
                .and_then(|value| self.resolve(value))
                .and_then(|value| numbers(std::slice::from_ref(value)).first().copied())
        };
        if let Some(width) = number(b"LW") {
            state.line_width = width;
        }
        if let Some(cap) = number(b"LC") {
            state.cap = cap as i64;
        }
        if let Some(join) = number(b"LJ") {
            state.join = join as i64;
        }
        if let Some(miter) = number(b"ML") {
            state.miter = miter;
        }
        if let Some(alpha) = number(b"CA") {
            state.stroke_alpha = alpha.clamp(0., 1.);
        }
        if let Some(alpha) = number(b"ca") {
            state.fill_alpha = alpha.clamp(0., 1.);
        }
        if let Some(Object::Array(dash)) = dict.get(b"D").ok().and_then(|d| self.resolve(d))
            && let (Some(Object::Array(array)), Some(phase)) = (dash.first(), dash.get(1))
        {
            state.dash = numbers(array);
            state.dash_phase = numbers(std::slice::from_ref(phase))
                .first()
                .copied()
                .unwrap_or(0.);
        }
    }

    fn paint_path(
        &mut self,
        path: &PathBuilder,
        state: &GraphicsState,
        fill: bool,
        stroke: bool,
        even_odd: bool,
    ) {
        let fill_colour = fill.then_some(state.fill).flatten();
        let stroke_colour = stroke.then_some(state.stroke).flatten();
        if fill_colour.is_none() && stroke_colour.is_none() {
            return;
        }
        self.order += 1;
        if fill_colour.is_some()
            && state.fill_alpha >= 1.
            && let Some(bounds) = bounds_of(&path.points)
        {
            // Clipped, it covers no more than its clip's rectangle.
            let mut bounds = bounds;
            for clip in &state.clips {
                if let Some([l, b, r, t]) = clip.rect {
                    bounds = [
                        bounds[0].max(l),
                        bounds[1].max(b),
                        bounds[2].min(r),
                        bounds[3].min(t),
                    ];
                }
            }
            if bounds[2] > bounds[0] && bounds[3] > bounds[1] {
                self.covers.push(Cover {
                    order: self.order,
                    bounds,
                    subpaths: path.outline.clone(),
                    even_odd,
                });
            }
        }
        // Clipped away entirely: nothing to draw.
        if clipped_away(&state.clips, &path.points) {
            return;
        }
        if !(path.segments <= 2 || path.rect().is_some()) {
            self.shapes += 1;
        }
        self.svg.group(&state.clips);
        let out = &mut self.svg.body;
        let m = state.ctm;
        out.push_str("<path d=\"");
        out.push_str(path.user.trim_end());
        out.push_str("\" transform=\"matrix(");
        for (index, value) in m.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            push_num(out, *value);
        }
        out.push_str(")\"");
        match fill_colour {
            Some(colour) => {
                let _ = write!(out, " fill=\"{}\"", hex(colour));
                if even_odd {
                    out.push_str(" fill-rule=\"evenodd\"");
                }
                if state.fill_alpha < 1. {
                    out.push_str(" fill-opacity=\"");
                    push_num(out, state.fill_alpha);
                    out.push('"');
                }
            }
            None => out.push_str(" fill=\"none\""),
        }
        if let Some(colour) = stroke_colour {
            let scale = (m[0] * m[3] - m[1] * m[2]).abs().sqrt().max(1e-9);
            let width = state.line_width.max(THINNEST_STROKE / scale);
            let _ = write!(out, " stroke=\"{}\" stroke-width=\"", hex(colour));
            push_num(out, width);
            out.push('"');
            match state.cap {
                1 => out.push_str(" stroke-linecap=\"round\""),
                2 => out.push_str(" stroke-linecap=\"square\""),
                _ => {}
            }
            match state.join {
                1 => out.push_str(" stroke-linejoin=\"round\""),
                2 => out.push_str(" stroke-linejoin=\"bevel\""),
                _ => {
                    if state.miter != 4. {
                        out.push_str(" stroke-miterlimit=\"");
                        push_num(out, state.miter.max(1.));
                        out.push('"');
                    }
                }
            }
            if !state.dash.is_empty() && state.dash.iter().any(|length| *length > 0.) {
                out.push_str(" stroke-dasharray=\"");
                for (index, length) in state.dash.iter().enumerate() {
                    if index > 0 {
                        out.push(' ');
                    }
                    push_num(out, length.max(0.));
                }
                out.push('"');
                if state.dash_phase != 0. {
                    out.push_str(" stroke-dashoffset=\"");
                    push_num(out, state.dash_phase);
                    out.push('"');
                }
            }
            if state.stroke_alpha < 1. {
                out.push_str(" stroke-opacity=\"");
                push_num(out, state.stroke_alpha);
                out.push('"');
            }
        }
        out.push_str("/>");
    }

    fn draw_xobject(
        &mut self,
        resources: &Dictionary,
        name: &[u8],
        state: &GraphicsState,
        depth: usize,
    ) {
        let Some(Object::Reference(id)) = self.resource(resources, b"XObject", name).cloned()
        else {
            return;
        };
        let Ok(Object::Stream(stream)) = self.document.get_object(id) else {
            return;
        };
        match stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|s| s.as_name().ok())
        {
            Some(b"Image") => self.image(id, stream, state),
            Some(b"Form") => {
                if depth >= MAX_FORM_DEPTH || !self.forms.insert(id) {
                    return;
                }
                self.form(stream, resources, state, depth);
                self.forms.remove(&id);
            }
            _ => {}
        }
    }

    fn form(
        &mut self,
        stream: &Stream,
        parent_resources: &Dictionary,
        state: &GraphicsState,
        depth: usize,
    ) {
        let mut inner = state.clone();
        if let Ok(Object::Array(matrix)) = stream.dict.get(b"Matrix") {
            let v = numbers(matrix);
            if v.len() == 6 {
                inner.ctm = multiply([v[0], v[1], v[2], v[3], v[4], v[5]], inner.ctm);
            }
        }
        if let Some(Object::Array(bbox)) = stream
            .dict
            .get(b"BBox")
            .ok()
            .and_then(|bbox| self.resolve(bbox))
        {
            let v = numbers(bbox);
            if v.len() == 4 {
                let corners = [(v[0], v[1]), (v[2], v[1]), (v[2], v[3]), (v[0], v[3])]
                    .map(|(x, y)| apply(inner.ctm, x, y));
                let mut data = String::new();
                for (index, (x, y)) in corners.iter().enumerate() {
                    data.push(if index == 0 { 'M' } else { 'L' });
                    push_num(&mut data, *x);
                    data.push(' ');
                    push_num(&mut data, *y);
                    data.push(' ');
                }
                data.push('Z');
                inner.clips.push(Clip {
                    path: data,
                    even_odd: false,
                    rect: rect_of(&corners),
                });
            }
        }
        let resources = stream
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|resources| self.dict(resources))
            .cloned()
            .unwrap_or_else(|| parent_resources.clone());
        let Ok(content) = stream
            .decompressed_content()
            .or_else(|_| Ok::<_, lopdf::Error>(stream.content.clone()))
        else {
            return;
        };
        if let Ok(content) = Content::decode(&content) {
            self.run(&content.operations, &resources, inner, depth + 1);
        }
    }

    fn image(&mut self, id: ObjectId, stream: &Stream, state: &GraphicsState) {
        let m = state.ctm;
        let corners = [(0., 0.), (1., 0.), (1., 1.), (0., 1.)].map(|(x, y)| apply(m, x, y));
        if clipped_away(&state.clips, &corners) {
            return;
        }
        let stencil = stream
            .dict
            .get(b"ImageMask")
            .ok()
            .and_then(|mask| mask.as_bool().ok())
            == Some(true);
        // Turned by quarters (mirrored at most) and clipped to rectangles
        // at most, or at any angle and not cut by a clip: a picture of its
        // own, which a word processor turns, mirrors and crops.
        let quarter = (m[1].abs() < 1e-6 && m[2].abs() < 1e-6 && m[0] != 0. && m[3] != 0.)
            || (m[0].abs() < 1e-6 && m[3].abs() < 1e-6 && m[1] != 0. && m[2] != 0.);
        let uncut = state.clips.iter().all(|clip| {
            clip.rect.is_some_and(|[l, b, r, t]| {
                corners.iter().all(|(x, y)| {
                    *x >= l - 0.01 && *x <= r + 0.01 && *y >= b - 0.01 && *y <= t + 0.01
                })
            })
        });
        let rectangular = state.clips.iter().all(|clip| clip.rect.is_some());
        if !stencil && ((quarter && rectangular) || uncut) {
            let rect = bounds_of(&corners).expect("four corners");
            let mut visible = rect;
            for clip in &state.clips {
                let [l, b, r, t] = clip.rect.expect("rectangular");
                visible = [
                    visible[0].max(l),
                    visible[1].max(b),
                    visible[2].min(r),
                    visible[3].min(t),
                ];
            }
            if visible[2] - visible[0] < 0.01 || visible[3] - visible[1] < 0.01 {
                return;
            }
            // The visible rectangle in the image's own space: what the crop
            // keeps of the image as stored.
            let det = m[0] * m[3] - m[1] * m[2];
            if det.abs() < 1e-12 {
                return;
            }
            let inverse = |x: f64, y: f64| {
                let (dx, dy) = (x - m[4], y - m[5]);
                (
                    (m[3] * dx - m[2] * dy) / det,
                    (-m[1] * dx + m[0] * dy) / det,
                )
            };
            let image_corners = [
                (visible[0], visible[1]),
                (visible[2], visible[1]),
                (visible[2], visible[3]),
                (visible[0], visible[3]),
            ]
            .map(|(x, y)| inverse(x, y));
            let [u0, v0, u1, v1] = bounds_of(&image_corners).expect("four corners");
            let cut = [
                u0.clamp(0., 1.),
                (1. - v1).clamp(0., 1.),
                (1. - u1).clamp(0., 1.),
                v0.clamp(0., 1.),
            ];
            let crop = cut.iter().any(|fraction| *fraction > 1e-4).then_some(cut);
            let over_paper = self.layers.is_empty() && !self.svg.painted;
            self.finish_layer();
            self.layers.push(GraphicsLayer::Picture(PlacedImage {
                object: id,
                matrix: m,
                crop,
                over_paper,
            }));
            return;
        }
        let encoded = if stencil {
            stencil_png(stream, self.document, state.fill.unwrap_or([0, 0, 0]))
                .map(|png| (png, "png"))
        } else {
            (self.encode)(id)
        };
        if let Some((bytes, extension)) = encoded {
            self.embed_image(&bytes, extension, state);
        }
    }

    fn inline_image(&mut self, stream: &Stream, resources: &Dictionary, state: &GraphicsState) {
        let Some(png) = inline_image_png(stream, self, resources, state) else {
            return;
        };
        self.embed_image(&png, "png", state);
    }

    /// An image in the SVG, drawn over the unit square of its matrix.
    fn embed_image(&mut self, bytes: &[u8], extension: &str, state: &GraphicsState) {
        self.embedded_images += 1;
        self.svg.group(&state.clips);
        let m = multiply([1., 0., 0., -1., 0., 1.], state.ctm);
        let out = &mut self.svg.body;
        out.push_str(
            "<image width=\"1\" height=\"1\" preserveAspectRatio=\"none\" transform=\"matrix(",
        );
        for (index, value) in m.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            let _ = write!(out, "{value:.6}");
        }
        let mime = if extension == "jpeg" {
            "image/jpeg"
        } else {
            "image/png"
        };
        let _ = write!(out, ")\" xlink:href=\"data:{mime};base64,");
        base64::engine::general_purpose::STANDARD.encode_string(bytes, out);
        out.push_str("\"/>");
    }

    /// Annotations' normal appearances, drawn over the content as viewers
    /// draw them; hidden ones and those without an appearance are not.
    fn annotations(&mut self, page_id: ObjectId) {
        let Ok(page) = self.document.get_dictionary(page_id) else {
            return;
        };
        let Some(Object::Array(annotations)) = page
            .get(b"Annots")
            .ok()
            .and_then(|annots| self.resolve(annots))
            .cloned()
        else {
            return;
        };
        for annotation in annotations {
            let Some(dict) = self.dict(&annotation).cloned() else {
                continue;
            };
            let flags = dict
                .get(b"F")
                .ok()
                .and_then(|flags| flags.as_i64().ok())
                .unwrap_or(0);
            // Hidden or not viewed.
            if flags & (2 | 32) != 0 {
                continue;
            }
            if matches!(
                dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()),
                Some(b"Popup" | b"Link" | b"Widget")
            ) {
                continue;
            }
            let Some(appearance) = dict
                .get(b"AP")
                .ok()
                .and_then(|ap| self.dict(ap))
                .and_then(|ap| ap.get(b"N").ok())
                .cloned()
            else {
                continue;
            };
            let appearance = match self.resolve(&appearance) {
                Some(Object::Dictionary(states)) => {
                    let Some(Object::Name(state)) = dict.get(b"AS").ok() else {
                        continue;
                    };
                    match states.get(state) {
                        Ok(object) => object.clone(),
                        Err(_) => continue,
                    }
                }
                _ => appearance,
            };
            let Object::Reference(id) = appearance else {
                continue;
            };
            let Ok(Object::Stream(stream)) = self.document.get_object(id) else {
                continue;
            };
            let rect = dict
                .get(b"Rect")
                .ok()
                .and_then(|rect| self.resolve(rect))
                .map(|rect| numbers(rect.as_array().map(Vec::as_slice).unwrap_or(&[])))
                .unwrap_or_default();
            if rect.len() != 4 {
                continue;
            }
            let (left, right) = (rect[0].min(rect[2]), rect[0].max(rect[2]));
            let (bottom, top) = (rect[1].min(rect[3]), rect[1].max(rect[3]));
            // The appearance's box, transformed, is fitted to the rectangle.
            let form_matrix = match stream.dict.get(b"Matrix").ok() {
                Some(Object::Array(matrix)) => {
                    let v = numbers(matrix);
                    if v.len() == 6 {
                        [v[0], v[1], v[2], v[3], v[4], v[5]]
                    } else {
                        IDENTITY
                    }
                }
                _ => IDENTITY,
            };
            let bbox = stream
                .dict
                .get(b"BBox")
                .ok()
                .and_then(|bbox| self.resolve(bbox))
                .map(|bbox| numbers(bbox.as_array().map(Vec::as_slice).unwrap_or(&[])))
                .unwrap_or_default();
            if bbox.len() != 4 {
                continue;
            }
            let corners = [
                (bbox[0], bbox[1]),
                (bbox[2], bbox[1]),
                (bbox[2], bbox[3]),
                (bbox[0], bbox[3]),
            ]
            .map(|(x, y)| apply(form_matrix, x, y));
            let Some([bl, bb, br, bt]) = bounds_of(&corners) else {
                continue;
            };
            if br - bl < 1e-6 || bt - bb < 1e-6 {
                continue;
            }
            let sx = (right - left) / (br - bl);
            let sy = (top - bottom) / (bt - bb);
            let state = GraphicsState {
                ctm: [sx, 0., 0., sy, left - bl * sx, bottom - bb * sy],
                ..GraphicsState::default()
            };
            if !self.forms.insert(id) {
                continue;
            }
            let resources =
                crate::pdf_text_edit::page_resources(self.document, page_id).unwrap_or_default();
            let stream = stream.clone();
            self.form(&stream, &resources, &state, 0);
            self.forms.remove(&id);
        }
    }

    /// Ends the SVG layer being written, if it painted anything.
    fn finish_layer(&mut self) {
        let layer = std::mem::take(&mut self.svg);
        if layer.painted {
            self.layers
                .push(GraphicsLayer::Svg(layer.document(self.page_box)));
        }
    }
}

fn bounds_of(points: &[(f64, f64)]) -> Option<[f64; 4]> {
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

/// The bounds of `points` when they are the corners of an axis-aligned
/// rectangle.
fn rect_of(points: &[(f64, f64)]) -> Option<[f64; 4]> {
    if points.len() != 4 {
        return None;
    }
    let bounds = bounds_of(points)?;
    let on_edge = |value: f64, a: f64, b: f64| (value - a).abs() < 0.01 || (value - b).abs() < 0.01;
    points
        .iter()
        .all(|(x, y)| on_edge(*x, bounds[0], bounds[2]) && on_edge(*y, bounds[1], bounds[3]))
        .then_some(bounds)
}

/// Whether a rectangular clip leaves nothing of the shape whose page-space
/// points are `points` (judged by their bounds).
fn clipped_away(clips: &[Clip], points: &[(f64, f64)]) -> bool {
    let Some([l, b, r, t]) = bounds_of(points) else {
        return true;
    };
    clips.iter().any(|clip| {
        clip.rect.is_some_and(|[cl, cb, cr, ct]| {
            r < cl - 0.5 || l > cr + 0.5 || t < cb - 0.5 || b > ct + 0.5
        })
    })
}

// --- Images drawn into the SVG -------------------------------------------

/// A stencil mask (`/ImageMask true`) as a PNG: `colour` where it paints,
/// transparent elsewhere.
fn stencil_png(stream: &Stream, document: &Document, colour: [u8; 3]) -> Option<Vec<u8>> {
    let dict = &stream.dict;
    let number = |key: &[u8]| {
        dict.get(key)
            .ok()
            .and_then(|value| document.dereference(value).ok())
            .and_then(|(_, value)| value.as_i64().ok())
    };
    let (width, height) = (number(b"Width")? as u32, number(b"Height")? as u32);
    let data = stream
        .decompressed_content()
        .ok()
        .unwrap_or_else(|| stream.content.clone());
    // `/Decode [1 0]` paints where the samples are one.
    let inverted = dict
        .get(b"Decode")
        .ok()
        .and_then(|decode| decode.as_array().ok())
        .and_then(|decode| decode.first())
        .and_then(|first| first.as_i64().ok())
        == Some(1);
    stencil_pixels(&data, width, height, inverted, colour)
}

fn stencil_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    inverted: bool,
    colour: [u8; 3],
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 50_000_000 {
        return None;
    }
    let row_bytes = (width as usize).div_ceil(8);
    if data.len() < row_bytes * height as usize {
        return None;
    }
    let mut rgba = image::RgbaImage::new(width, height);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let bit = (data[y * row_bytes + x / 8] >> (7 - x % 8)) & 1;
            let paints = (bit == 0) != inverted;
            if paints {
                rgba.put_pixel(
                    x as u32,
                    y as u32,
                    image::Rgba([colour[0], colour[1], colour[2], 255]),
                );
            }
        }
    }
    png(&rgba)
}

fn png(rgba: &image::RgbaImage) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut bytes),
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;
    Some(bytes)
}

/// An inline image (`BI … ID … EI`) as a PNG, in the device colours its
/// space gives; stencil masks paint the fill colour.
fn inline_image_png(
    stream: &Stream,
    painter: &Painter,
    resources: &Dictionary,
    state: &GraphicsState,
) -> Option<Vec<u8>> {
    let dict = &stream.dict;
    let get = |long: &[u8], short: &[u8]| dict.get(long).or_else(|_| dict.get(short)).ok();
    let width = get(b"Width", b"W")?.as_i64().ok()? as u32;
    let height = get(b"Height", b"H")?.as_i64().ok()? as u32;
    let mask = get(b"ImageMask", b"IM")
        .and_then(|mask| mask.as_bool().ok())
        .unwrap_or(false);
    let mut normalised = Stream::new(Dictionary::new(), stream.content.clone());
    if let Some(filter) = get(b"Filter", b"F") {
        let expand = |name: &[u8]| -> Vec<u8> {
            match name {
                b"Fl" => b"FlateDecode".to_vec(),
                b"AHx" => b"ASCIIHexDecode".to_vec(),
                b"A85" => b"ASCII85Decode".to_vec(),
                b"LZW" => b"LZWDecode".to_vec(),
                b"RL" => b"RunLengthDecode".to_vec(),
                b"DCT" => b"DCTDecode".to_vec(),
                other => other.to_vec(),
            }
        };
        let filter = match filter {
            Object::Name(name) => Object::Name(expand(name)),
            Object::Array(names) => Object::Array(
                names
                    .iter()
                    .filter_map(|name| name.as_name().ok())
                    .map(|name| Object::Name(expand(name)))
                    .collect(),
            ),
            _ => return None,
        };
        normalised.dict.set("Filter", filter);
    }
    let dct = matches!(
        normalised.dict.get(b"Filter").ok(),
        Some(Object::Name(name)) if name == b"DCTDecode"
    );
    if dct {
        let rgb = image::load_from_memory_with_format(&stream.content, image::ImageFormat::Jpeg)
            .ok()?
            .to_rgba8();
        return png(&rgb);
    }
    let data = if normalised.dict.get(b"Filter").is_ok() {
        normalised.decompressed_content().ok()?
    } else {
        stream.content.clone()
    };
    if mask {
        let inverted = get(b"Decode", b"D")
            .and_then(|decode| decode.as_array().ok())
            .and_then(|decode| decode.first())
            .and_then(|first| first.as_i64().ok())
            == Some(1);
        return stencil_pixels(
            &data,
            width,
            height,
            inverted,
            state.fill.unwrap_or([0, 0, 0]),
        );
    }
    let space = get(b"ColorSpace", b"CS")
        .map(|space| painter.colour_space(resources, space))
        .unwrap_or(ColourSpace::Gray);
    let bits = get(b"BitsPerComponent", b"BPC")
        .and_then(|bits| bits.as_i64().ok())
        .unwrap_or(8) as usize;
    let components = match &space {
        ColourSpace::Gray | ColourSpace::Indexed(_) | ColourSpace::Tint => 1,
        ColourSpace::Cmyk => 4,
        _ => 3,
    };
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 50_000_000 {
        return None;
    }
    let row_bytes = (width as usize * components * bits).div_ceil(8);
    if data.len() < row_bytes * height as usize || !matches!(bits, 1 | 2 | 4 | 8) {
        return None;
    }
    let max = ((1u32 << bits) - 1) as f64;
    let mut rgba = image::RgbaImage::new(width, height);
    for y in 0..height as usize {
        let row = &data[y * row_bytes..(y + 1) * row_bytes];
        for x in 0..width as usize {
            let mut values = [0f64; 4];
            for (c, value) in values.iter_mut().enumerate().take(components) {
                let index = (x * components + c) * bits;
                let byte = row[index / 8];
                let sample = if bits == 8 {
                    u32::from(byte)
                } else {
                    u32::from((byte >> (8 - bits - index % 8)) & ((1u16 << bits) - 1) as u8)
                };
                *value = if matches!(space, ColourSpace::Indexed(_)) {
                    f64::from(sample)
                } else {
                    f64::from(sample) / max
                };
            }
            let [r, g, b] = space.colour(&values[..components]).unwrap_or([0; 3]);
            rgba.put_pixel(x as u32, y as u32, image::Rgba([r, g, b, 255]));
        }
    }
    png(&rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn one_page(content: &str) -> (Document, ObjectId) {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let content_id =
            document.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog);
        (document, page_id)
    }

    fn svg_of(content: &str) -> String {
        let (document, page_id) = one_page(content);
        let graphics = page_graphics(&document, page_id, [0., 0., 200., 100.], &|_| None);
        match graphics.layers.as_slice() {
            [GraphicsLayer::Svg(svg)] => svg.clone(),
            other => panic!("one SVG layer, not {other:?}"),
        }
    }

    #[test]
    fn paths_keep_their_colours_widths_and_dashes_in_page_space() {
        let svg = svg_of("1 0 0 rg 0 0 1 RG 2 w [3 1] 0 d 10 10 50 20 re B");
        assert!(svg.contains("viewBox=\"0 0 200 100\""), "{svg}");
        assert!(svg.contains("matrix(1 0 0 -1 0 100)"), "{svg}");
        assert!(svg.contains("d=\"M10 10 L60 10 L60 30 L10 30 Z\""), "{svg}");
        assert!(svg.contains("fill=\"#ff0000\""), "{svg}");
        assert!(
            svg.contains("stroke=\"#0000ff\" stroke-width=\"2\""),
            "{svg}"
        );
        assert!(svg.contains("stroke-dasharray=\"3 1\""), "{svg}");
    }

    #[test]
    fn a_zero_width_stroke_is_drawn_thin_not_hidden() {
        let svg = svg_of("0 w 0 0 m 100 0 l S");
        assert!(svg.contains("stroke-width=\"0.24\""), "{svg}");
    }

    #[test]
    fn clips_nest_and_are_reused() {
        let svg =
            svg_of("q 0 0 100 100 re W n q 10 10 20 20 re W* n 0 0 m 50 50 l S 0 0 m 40 0 l S Q Q");
        assert_eq!(svg.matches("<clipPath").count(), 2, "{svg}");
        assert!(svg.contains("clip-path=\"url(#c0)\"><path"), "{svg}");
        assert!(svg.contains("clip-rule=\"evenodd\""), "{svg}");
        // Both strokes share one clipped group.
        assert_eq!(
            svg.matches("<g clip-path=\"url(#c1)\">").count(),
            1,
            "{svg}"
        );
    }

    #[test]
    fn text_is_left_out_and_unpainted_pages_have_no_layer() {
        let (document, page_id) = one_page("BT /F1 12 Tf 10 10 Td (Hi) Tj ET 0 0 m 1 1 l n");
        let graphics = page_graphics(&document, page_id, [0., 0., 200., 100.], &|_| None);
        assert!(graphics.layers.is_empty());
    }

    fn with_image(content: &str) -> PageGraphics {
        let (mut document, page_id) = one_page(content);
        let image = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            },
            vec![0; 12],
        ));
        let page = document
            .get_object_mut(page_id)
            .unwrap()
            .as_dict_mut()
            .unwrap();
        page.set(
            "Resources",
            dictionary! { "XObject" => dictionary! { "Im" => image } },
        );
        page_graphics(&document, page_id, [0., 0., 200., 100.], &|_| None)
    }

    #[test]
    fn an_image_turned_a_quarter_and_clipped_is_a_cropped_picture() {
        // The image's unit square turned a quarter (its x axis up the
        // page), clipped by a rectangle drawn as a closed polyline.
        let graphics = with_image(
            "q 0 0 m 50 0 l 50 100 l 0 100 l 0 0 l W n q 0 80 -40 0 60 10 cm /Im Do Q Q",
        );
        let [GraphicsLayer::Picture(image)] = graphics.layers.as_slice() else {
            panic!("one picture, not {:?}", graphics.layers);
        };
        assert_eq!(image.matrix, [0., 80., -40., 0., 60., 10.]);
        // The image spans x 20..60; the clip keeps x 20..50, which is the
        // image's top quarter (v from 0.25 to 1) as stored, its rows
        // running from its top.
        let [l, t, r, b] = image.crop.expect("cropped");
        assert!(
            l.abs() < 1e-9 && t.abs() < 1e-9 && r.abs() < 1e-9,
            "{:?}",
            image.crop
        );
        assert!((b - 0.25).abs() < 1e-9, "{:?}", image.crop);
        assert!(image.over_paper);
    }

    #[test]
    fn an_image_clipped_to_a_shape_goes_into_the_svg() {
        let graphics = with_image("q 0 0 m 50 0 l 25 50 l h W n q 40 0 0 40 0 0 cm /Im Do Q Q");
        assert!(
            graphics
                .layers
                .iter()
                .all(|layer| matches!(layer, GraphicsLayer::Svg(_))),
            "{:?}",
            graphics.layers
        );
    }

    #[test]
    fn a_fill_painted_after_text_covers_it() {
        let (document, page_id) = one_page(
            "BT /F1 12 Tf 20 30 Td (Under) Tj ET 1 g 10 20 80 30 re f \
             BT /F1 12 Tf 120 30 Td (Over) Tj ET",
        );
        let graphics = page_graphics(&document, page_id, [0., 0., 200., 100.], &|_| None);
        let marks = &graphics.text_marks;
        assert_eq!(marks.len(), 2);
        assert_eq!((marks[0].x, marks[0].y), (20., 30.));
        assert!(graphics.covered_after(marks[0].order, &[(25., 33.), (60., 33.)]));
        assert!(!graphics.covered_after(marks[1].order, &[(125., 33.)]));
        // Only what lies inside the fill is covered.
        assert!(!graphics.covered_after(marks[0].order, &[(25., 33.), (95., 33.)]));
    }

    #[test]
    fn numbers_are_short() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(1.23456), "1.235");
        assert_eq!(num(-0.5), "-0.5");
        assert_eq!(num(0.0001), "0");
    }
}
