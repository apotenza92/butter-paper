//! Bounded extraction of page-content geometry used by semantic snapping.
//!
//! This intentionally mirrors the stable Electron extractor's small graphics
//! subset. It does not attempt to render PDF content: straight path segments,
//! curve endpoints, rectangles, and closed paths are enough to build the
//! application's snap candidates. Each `/Contents` stream starts with fresh
//! graphics, path, and marked-content state, matching the reference parser.

use std::{collections::HashSet, error::Error, fmt};

use lopdf::{
    DecompressError, Document, Object, ObjectId, Stream,
    content::{Content, Operation},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PdfPoint {
    pub x: f64,
    pub y: f64,
}

impl PdfPoint {
    const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PdfRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PdfContentPrimitive {
    Line { start: PdfPoint, end: PdfPoint },
    Rect { rect: PdfRect },
    Polyline { points: Vec<PdfPoint>, closed: bool },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageSnapGeometry {
    pub page_index: u32,
    pub primitives: Vec<PdfContentPrimitive>,
}

/// Resource limits for one page-content geometry request.
///
/// The decompression limits bound hostile filter expansion before `lopdf`
/// allocates decoded output. The remaining limits bound parser and protocol
/// output growth after decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentGeometryLimits {
    pub max_content_streams: usize,
    pub max_reference_depth: usize,
    pub max_decoded_bytes_per_stream: usize,
    pub max_decoded_bytes_per_page: usize,
    pub max_operations_per_stream: usize,
    pub max_operations_per_page: usize,
    pub max_primitives_per_page: usize,
    pub max_path_points: usize,
    pub max_graphics_state_depth: usize,
    pub max_marked_content_depth: usize,
}

impl Default for ContentGeometryLimits {
    fn default() -> Self {
        Self {
            max_content_streams: 256,
            max_reference_depth: 32,
            max_decoded_bytes_per_stream: 8 * 1024 * 1024,
            max_decoded_bytes_per_page: 16 * 1024 * 1024,
            max_operations_per_stream: 250_000,
            max_operations_per_page: 250_000,
            max_primitives_per_page: 50_000,
            max_path_points: 100_000,
            max_graphics_state_depth: 256,
            max_marked_content_depth: 256,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentGeometryError {
    LimitExceeded(String),
    Page(String),
    Malformed(String),
}

impl fmt::Display for ContentGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded(detail) => {
                write!(formatter, "content geometry limit exceeded: {detail}")
            }
            Self::Page(detail) => write!(formatter, "page error: {detail}"),
            Self::Malformed(detail) => write!(formatter, "malformed page content: {detail}"),
        }
    }
}

impl Error for ContentGeometryError {}

type Matrix = [f64; 6];

const IDENTITY_MATRIX: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[derive(Clone, Copy)]
struct GraphicsState {
    ctm: Matrix,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: IDENTITY_MATRIX,
        }
    }
}

#[derive(Default)]
struct PathState {
    points: Vec<PdfPoint>,
    start: Option<PdfPoint>,
    current: Option<PdfPoint>,
}

struct ExtractionState<'a> {
    limits: &'a ContentGeometryLimits,
    primitives: Vec<PdfContentPrimitive>,
    decoded_bytes: usize,
    operation_count: usize,
}

/// Extracts zero-based `page_index` content geometry from the immutable
/// metadata document owned by the PDF worker.
pub fn extract_page_snap_geometry(
    document: &Document,
    page_index: u32,
    limits: ContentGeometryLimits,
) -> Result<PageSnapGeometry, ContentGeometryError> {
    validate_limits(&limits)?;
    let page_number = page_index.checked_add(1).ok_or_else(|| {
        ContentGeometryError::Page(format!("page index {page_index} cannot be represented"))
    })?;
    let pages = document.get_pages();
    let page_id = *pages.get(&page_number).ok_or_else(|| {
        ContentGeometryError::Page(format!(
            "page index {page_index} is outside the {}-page document",
            pages.len()
        ))
    })?;
    let page = document.get_dictionary(page_id).map_err(|error| {
        ContentGeometryError::Malformed(format!(
            "page {page_number} dictionary is unavailable: {error}"
        ))
    })?;

    let mut streams = Vec::new();
    if let Ok(contents) = page.get(b"Contents") {
        collect_content_streams(
            document,
            contents,
            0,
            &limits,
            &mut HashSet::new(),
            &mut streams,
        )?;
    }

    let mut state = ExtractionState {
        limits: &limits,
        primitives: Vec::new(),
        decoded_bytes: 0,
        operation_count: 0,
    };
    for stream in streams {
        parse_content_stream(stream, &mut state)?;
    }

    Ok(PageSnapGeometry {
        page_index,
        primitives: state.primitives,
    })
}

fn validate_limits(limits: &ContentGeometryLimits) -> Result<(), ContentGeometryError> {
    let values = [
        ("content streams", limits.max_content_streams),
        ("reference depth", limits.max_reference_depth),
        (
            "decoded bytes per stream",
            limits.max_decoded_bytes_per_stream,
        ),
        ("decoded bytes per page", limits.max_decoded_bytes_per_page),
        ("operations per stream", limits.max_operations_per_stream),
        ("operations per page", limits.max_operations_per_page),
        ("primitives per page", limits.max_primitives_per_page),
        ("path points", limits.max_path_points),
        ("graphics-state depth", limits.max_graphics_state_depth),
        ("marked-content depth", limits.max_marked_content_depth),
    ];
    if let Some((name, _)) = values.into_iter().find(|(_, value)| *value == 0) {
        return Err(ContentGeometryError::LimitExceeded(format!(
            "configured {name} limit must be positive"
        )));
    }
    Ok(())
}

fn collect_content_streams<'a>(
    document: &'a Document,
    object: &'a Object,
    depth: usize,
    limits: &ContentGeometryLimits,
    reference_stack: &mut HashSet<ObjectId>,
    streams: &mut Vec<&'a Stream>,
) -> Result<(), ContentGeometryError> {
    if depth > limits.max_reference_depth {
        return Err(ContentGeometryError::LimitExceeded(format!(
            "content reference depth exceeds {}",
            limits.max_reference_depth
        )));
    }
    match object {
        Object::Reference(object_id) => {
            if !reference_stack.insert(*object_id) {
                return Err(ContentGeometryError::Malformed(format!(
                    "cyclic /Contents reference {} {}",
                    object_id.0, object_id.1
                )));
            }
            let resolved = document.get_object(*object_id).map_err(|error| {
                ContentGeometryError::Malformed(format!(
                    "cannot resolve /Contents object {} {}: {error}",
                    object_id.0, object_id.1
                ))
            })?;
            let result = collect_content_streams(
                document,
                resolved,
                depth + 1,
                limits,
                reference_stack,
                streams,
            );
            reference_stack.remove(object_id);
            result
        }
        Object::Array(entries) => {
            for entry in entries {
                collect_content_streams(
                    document,
                    entry,
                    depth + 1,
                    limits,
                    reference_stack,
                    streams,
                )?;
            }
            Ok(())
        }
        Object::Stream(stream) => {
            if streams.len() >= limits.max_content_streams {
                return Err(ContentGeometryError::LimitExceeded(format!(
                    "page has more than {} content streams",
                    limits.max_content_streams
                )));
            }
            streams.push(stream);
            Ok(())
        }
        other => Err(ContentGeometryError::Malformed(format!(
            "/Contents contains {}, expected a stream or array",
            object_kind(other)
        ))),
    }
}

fn parse_content_stream(
    stream: &Stream,
    state: &mut ExtractionState<'_>,
) -> Result<(), ContentGeometryError> {
    let decoded = stream
        .decompressed_content_with_limit(state.limits.max_decoded_bytes_per_stream)
        .map_err(|error| match error {
            lopdf::Error::Decompress(DecompressError::MemoryLimitExceeded { .. }) => {
                ContentGeometryError::LimitExceeded(format!(
                    "a content stream exceeds {} decoded bytes",
                    state.limits.max_decoded_bytes_per_stream
                ))
            }
            other => ContentGeometryError::Malformed(format!(
                "content stream cannot be decoded: {other}"
            )),
        })?;
    state.decoded_bytes = state
        .decoded_bytes
        .checked_add(decoded.len())
        .ok_or_else(|| {
            ContentGeometryError::LimitExceeded("decoded byte count overflowed".into())
        })?;
    if state.decoded_bytes > state.limits.max_decoded_bytes_per_page {
        return Err(ContentGeometryError::LimitExceeded(format!(
            "page content exceeds {} decoded bytes",
            state.limits.max_decoded_bytes_per_page
        )));
    }

    let content = Content::decode(&decoded).map_err(|error| {
        ContentGeometryError::Malformed(format!("content stream syntax is invalid: {error}"))
    })?;
    if content.operations.len() > state.limits.max_operations_per_stream {
        return Err(ContentGeometryError::LimitExceeded(format!(
            "a content stream has more than {} operations",
            state.limits.max_operations_per_stream
        )));
    }
    state.operation_count = state
        .operation_count
        .checked_add(content.operations.len())
        .ok_or_else(|| ContentGeometryError::LimitExceeded("operation count overflowed".into()))?;
    if state.operation_count > state.limits.max_operations_per_page {
        return Err(ContentGeometryError::LimitExceeded(format!(
            "page content has more than {} operations",
            state.limits.max_operations_per_page
        )));
    }

    // Deliberately reset all parser state for every source stream.
    let mut graphics_state = GraphicsState::default();
    let mut graphics_stack = Vec::new();
    let mut path = PathState::default();
    let mut artifact_stack = Vec::new();
    for operation in &content.operations {
        apply_operation(
            operation,
            state,
            &mut graphics_state,
            &mut graphics_stack,
            &mut path,
            &mut artifact_stack,
        )?;
    }
    Ok(())
}

fn apply_operation(
    operation: &Operation,
    state: &mut ExtractionState<'_>,
    graphics_state: &mut GraphicsState,
    graphics_stack: &mut Vec<GraphicsState>,
    path: &mut PathState,
    artifact_stack: &mut Vec<bool>,
) -> Result<(), ContentGeometryError> {
    let inside_artifact = artifact_stack.last().copied().unwrap_or(false);
    match operation.operator.as_str() {
        "BMC" | "BDC" => {
            if artifact_stack.len() >= state.limits.max_marked_content_depth {
                return Err(ContentGeometryError::LimitExceeded(format!(
                    "marked-content nesting exceeds {}",
                    state.limits.max_marked_content_depth
                )));
            }
            artifact_stack.push(inside_artifact || operation_contains_artifact(operation));
        }
        "EMC" => {
            artifact_stack.pop();
        }
        "q" => {
            if graphics_stack.len() >= state.limits.max_graphics_state_depth {
                return Err(ContentGeometryError::LimitExceeded(format!(
                    "graphics-state nesting exceeds {}",
                    state.limits.max_graphics_state_depth
                )));
            }
            graphics_stack.push(*graphics_state);
        }
        "Q" => {
            *graphics_state = graphics_stack.pop().unwrap_or_default();
        }
        "cm" => {
            if let Some(values) = last_numbers::<6>(&operation.operands) {
                graphics_state.ctm = multiply_matrix(graphics_state.ctm, values);
            }
        }
        "m" => {
            if let Some([x, y]) = last_numbers::<2>(&operation.operands) {
                let point = transform_point(graphics_state.ctm, x, y)?;
                *path = PathState {
                    points: vec![point],
                    start: Some(point),
                    current: Some(point),
                };
            }
        }
        "l" => {
            if let Some([x, y]) = last_numbers::<2>(&operation.operands) {
                let point = transform_point(graphics_state.ctm, x, y)?;
                if let Some(current) = path.current.filter(|_| !inside_artifact) {
                    push_primitive(
                        state,
                        PdfContentPrimitive::Line {
                            start: current,
                            end: point,
                        },
                    )?;
                }
                push_path_point(path, point, state.limits.max_path_points)?;
            }
        }
        "c" | "v" | "y" => {
            // Stable Butter Paper deliberately treats only a curve's endpoint
            // as a straight snap segment; control points are not snap geometry.
            if let Some([x, y]) = last_numbers::<2>(&operation.operands) {
                let point = transform_point(graphics_state.ctm, x, y)?;
                if let Some(current) = path.current.filter(|_| !inside_artifact) {
                    push_primitive(
                        state,
                        PdfContentPrimitive::Line {
                            start: current,
                            end: point,
                        },
                    )?;
                }
                push_path_point(path, point, state.limits.max_path_points)?;
            }
        }
        "h" => {
            if !inside_artifact {
                if let (Some(current), Some(start)) = (path.current, path.start) {
                    if !same_point(current, start) {
                        push_primitive(
                            state,
                            PdfContentPrimitive::Line {
                                start: current,
                                end: start,
                            },
                        )?;
                    }
                }
                if path.points.len() >= 3 {
                    let primitive = rect_from_closed_path(&path.points)
                        .map(|rect| PdfContentPrimitive::Rect { rect })
                        .unwrap_or_else(|| PdfContentPrimitive::Polyline {
                            points: path.points.clone(),
                            closed: true,
                        });
                    push_primitive(state, primitive)?;
                }
            }
            path.current = path.start;
        }
        "re" => {
            if let Some([x, y, width, height]) = last_numbers::<4>(&operation.operands) {
                let corners = [
                    transform_point(graphics_state.ctm, x, y)?,
                    transform_point(graphics_state.ctm, x + width, y)?,
                    transform_point(graphics_state.ctm, x + width, y + height)?,
                    transform_point(graphics_state.ctm, x, y + height)?,
                ];
                if !inside_artifact {
                    push_primitive(
                        state,
                        PdfContentPrimitive::Rect {
                            rect: rect_from_corners(&corners),
                        },
                    )?;
                }
                if corners.len() > state.limits.max_path_points {
                    return Err(ContentGeometryError::LimitExceeded(format!(
                        "path has more than {} points",
                        state.limits.max_path_points
                    )));
                }
                *path = PathState {
                    points: corners.to_vec(),
                    start: Some(corners[0]),
                    current: Some(corners[3]),
                };
            }
        }
        "S" | "s" | "B" | "B*" | "b" | "b*" | "n" | "f" | "F" | "f*" => {
            *path = PathState::default();
        }
        _ => {}
    }
    Ok(())
}

fn last_numbers<const N: usize>(operands: &[Object]) -> Option<[f64; N]> {
    let start = operands.len().checked_sub(N)?;
    let mut values = [0.0; N];
    for (index, operand) in operands[start..].iter().enumerate() {
        values[index] = object_number(operand)?;
    }
    Some(values)
}

fn object_number(object: &Object) -> Option<f64> {
    let value = match object {
        Object::Integer(value) => *value as f64,
        Object::Real(value) => f64::from(*value),
        _ => return None,
    };
    value.is_finite().then_some(value)
}

fn operation_contains_artifact(operation: &Operation) -> bool {
    operation
        .operands
        .iter()
        .any(|operand| matches!(operand, Object::Name(name) if name == b"Artifact"))
}

fn push_path_point(
    path: &mut PathState,
    point: PdfPoint,
    maximum: usize,
) -> Result<(), ContentGeometryError> {
    if path.points.len() >= maximum {
        return Err(ContentGeometryError::LimitExceeded(format!(
            "path has more than {maximum} points"
        )));
    }
    path.points.push(point);
    path.current = Some(point);
    Ok(())
}

fn push_primitive(
    state: &mut ExtractionState<'_>,
    primitive: PdfContentPrimitive,
) -> Result<(), ContentGeometryError> {
    if state.primitives.len() >= state.limits.max_primitives_per_page {
        return Err(ContentGeometryError::LimitExceeded(format!(
            "page has more than {} content primitives",
            state.limits.max_primitives_per_page
        )));
    }
    state.primitives.push(primitive);
    Ok(())
}

fn transform_point(matrix: Matrix, x: f64, y: f64) -> Result<PdfPoint, ContentGeometryError> {
    let [a, b, c, d, e, f] = matrix;
    let point = PdfPoint::new(a * x + c * y + e, b * x + d * y + f);
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(ContentGeometryError::Malformed(
            "content transform produced a non-finite coordinate".into(),
        ));
    }
    Ok(point)
}

fn multiply_matrix(left: Matrix, right: Matrix) -> Matrix {
    let [a1, b1, c1, d1, e1, f1] = left;
    let [a2, b2, c2, d2, e2, f2] = right;
    [
        a1 * a2 + c1 * b2,
        b1 * a2 + d1 * b2,
        a1 * c2 + c1 * d2,
        b1 * c2 + d1 * d2,
        a1 * e2 + c1 * f2 + e1,
        b1 * e2 + d1 * f2 + f1,
    ]
}

fn rect_from_corners(corners: &[PdfPoint; 4]) -> PdfRect {
    let min_x = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

fn rect_from_closed_path(points: &[PdfPoint]) -> Option<PdfRect> {
    if points.len() != 4 {
        return None;
    }
    let mut xs = Vec::with_capacity(2);
    let mut ys = Vec::with_capacity(2);
    for point in points {
        push_unique(&mut xs, round_coordinate(point.x));
        push_unique(&mut ys, round_coordinate(point.y));
    }
    if xs.len() != 2 || ys.len() != 2 {
        return None;
    }
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    Some(PdfRect {
        x: xs[0],
        y: ys[0],
        width: xs[1] - xs[0],
        height: ys[1] - ys[0],
    })
}

fn push_unique(values: &mut Vec<f64>, value: f64) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn round_coordinate(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn same_point(left: PdfPoint, right: PdfPoint) -> bool {
    (left.x - right.x).abs() < 0.0001 && (left.y - right.y).abs() < 0.0001
}

fn object_kind(object: &Object) -> &'static str {
    match object {
        Object::Null => "null",
        Object::Boolean(_) => "a boolean",
        Object::Integer(_) | Object::Real(_) => "a number",
        Object::Name(_) => "a name",
        Object::String(_, _) => "a string",
        Object::Array(_) => "an array",
        Object::Dictionary(_) => "a dictionary",
        Object::Stream(_) => "a stream",
        Object::Reference(_) => "a reference",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Object, Stream, dictionary};

    fn document_with_streams(streams: &[&[u8]]) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let contents = streams
            .iter()
            .map(|content| {
                Object::Reference(
                    document.add_object(Stream::new(dictionary! {}, content.to_vec())),
                )
            })
            .collect::<Vec<_>>();
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Resources" => dictionary! {},
            "Contents" => Object::Array(contents),
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document
    }

    fn extract(streams: &[&[u8]]) -> PageSnapGeometry {
        extract_page_snap_geometry(
            &document_with_streams(streams),
            0,
            ContentGeometryLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn composes_ctm_and_restores_graphics_state() {
        let geometry = extract(&[b"q 2 0 0 3 10 20 cm 1 2 m 4 6 l Q 1 1 m 2 2 l"]);
        assert_eq!(
            geometry.primitives,
            vec![
                PdfContentPrimitive::Line {
                    start: PdfPoint::new(12.0, 26.0),
                    end: PdfPoint::new(18.0, 38.0),
                },
                PdfContentPrimitive::Line {
                    start: PdfPoint::new(1.0, 1.0),
                    end: PdfPoint::new(2.0, 2.0),
                },
            ]
        );
    }

    #[test]
    fn excludes_artifact_marked_content_including_nested_content() {
        let geometry = extract(&[
            b"/Artifact BMC 0 0 m 10 0 l /Span BMC 10 0 m 20 0 l EMC EMC 20 20 m 30 20 l",
        ]);
        assert_eq!(
            geometry.primitives,
            vec![PdfContentPrimitive::Line {
                start: PdfPoint::new(20.0, 20.0),
                end: PdfPoint::new(30.0, 20.0),
            }]
        );
    }

    #[test]
    fn extracts_reference_path_operators_and_resets_painted_paths() {
        let geometry = extract(&[b"0 0 m 10 0 l 12 2 18 8 20 10 c 22 12 30 10 v 35 5 40 0 y h S 50 50 10 20 re f 90 90 m n 100 100 l"]);
        assert_eq!(geometry.primitives.len(), 7);
        assert_eq!(
            geometry.primitives[0],
            PdfContentPrimitive::Line {
                start: PdfPoint::new(0.0, 0.0),
                end: PdfPoint::new(10.0, 0.0),
            }
        );
        assert_eq!(
            geometry.primitives[1],
            PdfContentPrimitive::Line {
                start: PdfPoint::new(10.0, 0.0),
                end: PdfPoint::new(20.0, 10.0),
            }
        );
        assert_eq!(
            geometry.primitives[2],
            PdfContentPrimitive::Line {
                start: PdfPoint::new(20.0, 10.0),
                end: PdfPoint::new(30.0, 10.0),
            }
        );
        assert_eq!(
            geometry.primitives[3],
            PdfContentPrimitive::Line {
                start: PdfPoint::new(30.0, 10.0),
                end: PdfPoint::new(40.0, 0.0),
            }
        );
        assert_eq!(
            geometry.primitives[4],
            PdfContentPrimitive::Line {
                start: PdfPoint::new(40.0, 0.0),
                end: PdfPoint::new(0.0, 0.0),
            }
        );
        assert!(matches!(
            &geometry.primitives[5],
            PdfContentPrimitive::Polyline { points, closed: true }
                if points == &vec![
                    PdfPoint::new(0.0, 0.0),
                    PdfPoint::new(10.0, 0.0),
                    PdfPoint::new(20.0, 10.0),
                    PdfPoint::new(30.0, 10.0),
                    PdfPoint::new(40.0, 0.0),
                ]
        ));
        assert_eq!(
            geometry.primitives[6],
            PdfContentPrimitive::Rect {
                rect: PdfRect {
                    x: 50.0,
                    y: 50.0,
                    width: 10.0,
                    height: 20.0,
                },
            }
        );
    }

    #[test]
    fn resets_ctm_path_and_artifact_state_between_content_streams() {
        let geometry = extract(&[b"2 0 0 2 100 100 cm /Artifact BMC 0 0 m", b"1 1 m 2 2 l"]);
        assert_eq!(
            geometry.primitives,
            vec![PdfContentPrimitive::Line {
                start: PdfPoint::new(1.0, 1.0),
                end: PdfPoint::new(2.0, 2.0),
            }]
        );
    }

    #[test]
    fn enforces_explicit_stream_operation_primitive_and_path_limits() {
        let document = document_with_streams(&[b"0 0 m 1 1 l 2 2 l"]);
        let defaults = ContentGeometryLimits::default();

        let error = extract_page_snap_geometry(
            &document,
            0,
            ContentGeometryLimits {
                max_decoded_bytes_per_stream: 4,
                ..defaults
            },
        )
        .unwrap_err();
        assert!(matches!(error, ContentGeometryError::LimitExceeded(_)));

        let error = extract_page_snap_geometry(
            &document,
            0,
            ContentGeometryLimits {
                max_operations_per_stream: 1,
                ..defaults
            },
        )
        .unwrap_err();
        assert!(matches!(error, ContentGeometryError::LimitExceeded(_)));

        let error = extract_page_snap_geometry(
            &document,
            0,
            ContentGeometryLimits {
                max_primitives_per_page: 1,
                ..defaults
            },
        )
        .unwrap_err();
        assert!(matches!(error, ContentGeometryError::LimitExceeded(_)));

        let error = extract_page_snap_geometry(
            &document,
            0,
            ContentGeometryLimits {
                max_path_points: 2,
                ..defaults
            },
        )
        .unwrap_err();
        assert!(matches!(error, ContentGeometryError::LimitExceeded(_)));
    }

    #[test]
    fn reports_out_of_range_pages_separately_from_malformed_content() {
        let document = document_with_streams(&[b"0 0 m 1 1 l"]);
        assert!(matches!(
            extract_page_snap_geometry(&document, 1, ContentGeometryLimits::default()),
            Err(ContentGeometryError::Page(_))
        ));

        let mut malformed = document_with_streams(&[b"0 0 m 1 1 l"]);
        let page_id = *malformed.get_pages().get(&1).unwrap();
        malformed
            .get_dictionary_mut(page_id)
            .unwrap()
            .set("Contents", Object::Integer(7));
        assert!(matches!(
            extract_page_snap_geometry(&malformed, 0, ContentGeometryLimits::default()),
            Err(ContentGeometryError::Malformed(_))
        ));
    }
}
