//! Page content changed in Edit PDF mode: text lines, drawings and images,
//! each named by where the base page draws it, and moved, resized (drawings
//! and images) or removed.
//!
//! A changed piece of content is placed by its change alone: table edits
//! leave it where it was put.

use serde::{Deserialize, Serialize};

use crate::pdf_tables::{PageTableEdit, content_offset};

/// How close two bounds must be, in points, to name the same content.
const SAME_RECT: f64 = 0.01;
/// The farthest content may be moved, in points.
const MAX_OFFSET: f64 = 100_000.;
/// The least and most content may be scaled.
const MIN_SCALE: f64 = 0.001;
const MAX_SCALE: f64 = 1000.;

/// What a piece of moved content is.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    /// A text line.
    Text,
    /// A painted path.
    #[default]
    Drawing,
    /// An image or form drawn with `Do`.
    Image,
}

/// A piece of page content and how it changes: scaled from its bottom-left
/// corner, turned about its centre, then moved, or removed.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContentMove {
    pub page_index: u32,
    pub kind: ContentKind,
    /// Where the base page draws it (left, bottom, right, top): a text
    /// line's core box ([`crate::pdf_tables::line_core_box`]), a path's
    /// bounds or an image's.
    pub rect: [f64; 4],
    /// Which of the contents of its kind the page draws at the same `rect`
    /// it is, in paint order, from 0: a brace drawn corner to corner of its
    /// frame has the frame's bounds.
    #[serde(default, skip_serializing_if = "is_first")]
    pub nth: u32,
    /// How far it moves, in page space.
    pub offset: [f64; 2],
    /// How much wider and taller it is drawn; text is never scaled.
    #[serde(default = "unscaled", skip_serializing_if = "is_unscaled")]
    pub scale: [f64; 2],
    /// How far it is turned, in degrees anticlockwise, about the centre of
    /// its scaled bounds; text is never turned.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
    /// It is deleted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
    /// It is not the content itself but copy number `n` of it, drawn as
    /// well, placed by `offset` and `scale` from where the base page draws
    /// the content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<u32>,
}

fn is_first(nth: &u32) -> bool {
    *nth == 0
}

fn unscaled() -> [f64; 2] {
    [1., 1.]
}

fn is_unscaled(scale: &[f64; 2]) -> bool {
    *scale == [1., 1.]
}

/// How content is placed: scaled by `scale` from the bottom-left corner of
/// the bounds the page draws it at, turned by `rotation` degrees
/// anticlockwise about the centre of those scaled bounds, then moved by
/// `offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub offset: [f64; 2],
    pub scale: [f64; 2],
    pub rotation: f64,
}

impl Placement {
    pub const UNMOVED: Self = Self {
        offset: [0., 0.],
        scale: [1., 1.],
        rotation: 0.,
    };

    pub fn moved(offset: (f64, f64)) -> Self {
        Self {
            offset: [offset.0, offset.1],
            ..Self::UNMOVED
        }
    }

    /// The centre the content turns about: its scaled bounds' centre,
    /// before it is moved.
    fn pivot(&self, rect: [f64; 4]) -> (f64, f64) {
        (
            rect[0] + (rect[2] - rect[0]) * self.scale[0] / 2.,
            rect[1] + (rect[3] - rect[1]) * self.scale[1] / 2.,
        )
    }

    /// Where the content's centre is shown.
    pub fn centre(&self, rect: [f64; 4]) -> (f64, f64) {
        let (x, y) = self.pivot(rect);
        (x + self.offset[0], y + self.offset[1])
    }

    /// Where page point `point` of content the page draws at `rect` goes.
    pub fn map(&self, rect: [f64; 4], (x, y): (f64, f64)) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.matrix(rect);
        (a * x + c * y + e, b * x + d * y + f)
    }

    /// Where content the page draws at `rect` is shown: the upright bounds
    /// of its turned box.
    pub fn shown(&self, rect: [f64; 4]) -> [f64; 4] {
        let corners = [
            (rect[0], rect[1]),
            (rect[2], rect[1]),
            (rect[2], rect[3]),
            (rect[0], rect[3]),
        ]
        .map(|corner| self.map(rect, corner));
        corners.iter().fold(
            [f64::INFINITY, f64::INFINITY, -f64::INFINITY, -f64::INFINITY],
            |b, &(x, y)| [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
        )
    }

    /// The corners of its box as shown, from its bottom-left anticlockwise.
    pub fn shown_corners(&self, rect: [f64; 4]) -> [(f64, f64); 4] {
        [
            (rect[0], rect[1]),
            (rect[2], rect[1]),
            (rect[2], rect[3]),
            (rect[0], rect[3]),
        ]
        .map(|corner| self.map(rect, corner))
    }

    /// As a page-space matrix (`[a b c d e f]`, row vectors).
    pub fn matrix(&self, rect: [f64; 4]) -> [f64; 6] {
        let (sx, sy) = (self.scale[0], self.scale[1]);
        // Scaled from the bottom-left corner.
        let (tx, ty) = (rect[0] * (1. - sx), rect[1] * (1. - sy));
        if self.rotation == 0. {
            return [sx, 0., 0., sy, tx + self.offset[0], ty + self.offset[1]];
        }
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let (cx, cy) = self.pivot(rect);
        [
            cos * sx,
            sin * sx,
            -sin * sy,
            cos * sy,
            cx + cos * (tx - cx) - sin * (ty - cy) + self.offset[0],
            cy + sin * (tx - cx) + cos * (ty - cy) + self.offset[1],
        ]
    }

    /// This placement changed further by `change`: moved, or resized about
    /// a page point, as the content is shown now.
    pub fn changed(&self, rect: [f64; 4], change: ContentChange) -> Self {
        match change {
            ContentChange::Move(dx, dy) => Self {
                offset: [self.offset[0] + dx, self.offset[1] + dy],
                ..*self
            },
            ContentChange::Resize { origin, factor } if self.rotation == 0. => Self {
                scale: [self.scale[0] * factor.0, self.scale[1] * factor.1],
                offset: [
                    origin.0 + (rect[0] + self.offset[0] - origin.0) * factor.0 - rect[0],
                    origin.1 + (rect[1] + self.offset[1] - origin.1) * factor.1 - rect[1],
                ],
                ..*self
            },
            // Turned content is scaled in its own axes, so it never skews:
            // a quarter turn swaps the page's axes for its own, and any
            // other angle keeps its proportions. Its centre follows the
            // resize.
            ContentChange::Resize { origin, factor } => {
                let quarter = (self.rotation / 90.).round();
                let scale = if (self.rotation - quarter * 90.).abs() < 1e-6 {
                    if quarter.rem_euclid(2.) == 1. {
                        (factor.1, factor.0)
                    } else {
                        factor
                    }
                } else {
                    let uniform = if (factor.0 - 1.).abs() < 1e-9 {
                        factor.1
                    } else if (factor.1 - 1.).abs() < 1e-9 {
                        factor.0
                    } else {
                        factor.0.max(factor.1)
                    };
                    (uniform, uniform)
                };
                let (cx, cy) = self.centre(rect);
                let resized = Self {
                    scale: [self.scale[0] * scale.0, self.scale[1] * scale.1],
                    offset: [0., 0.],
                    ..*self
                };
                let (px, py) = resized.pivot(rect);
                let to = if scale.0 == scale.1 {
                    (
                        origin.0 + (cx - origin.0) * scale.0,
                        origin.1 + (cy - origin.1) * scale.0,
                    )
                } else {
                    (
                        origin.0 + (cx - origin.0) * factor.0,
                        origin.1 + (cy - origin.1) * factor.1,
                    )
                };
                Self {
                    offset: [to.0 - px, to.1 - py],
                    ..resized
                }
            }
            ContentChange::Rotate { pivot, degrees } => {
                let (cx, cy) = self.centre(rect);
                let (x, y) = change.map((cx, cy));
                let _ = pivot;
                Self {
                    rotation: normalized_degrees(self.rotation + degrees),
                    offset: [self.offset[0] + x - cx, self.offset[1] + y - cy],
                    ..*self
                }
            }
            ContentChange::Remove => *self,
        }
    }
}

/// A change to selected content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContentChange {
    /// Moved by an offset in page space.
    Move(f64, f64),
    /// Scaled by `factor` about the page point `origin`.
    Resize {
        origin: (f64, f64),
        factor: (f64, f64),
    },
    /// Turned `degrees` anticlockwise about the page point `pivot`.
    Rotate { pivot: (f64, f64), degrees: f64 },
    /// Deleted.
    Remove,
}

/// `degrees` as an angle from -180 (exclusive) to 180, 0 when within a
/// millionth of a degree of a whole turn.
pub fn normalized_degrees(degrees: f64) -> f64 {
    let turned = (degrees + 180.).rem_euclid(360.) - 180.;
    let turned = if turned <= -180. {
        turned + 360.
    } else {
        turned
    };
    if turned.abs() < 1e-6 { 0. } else { turned }
}

impl ContentChange {
    /// Where a page point moves.
    pub fn map(&self, (x, y): (f64, f64)) -> (f64, f64) {
        match *self {
            Self::Move(dx, dy) => (x + dx, y + dy),
            Self::Resize { origin, factor } => (
                origin.0 + (x - origin.0) * factor.0,
                origin.1 + (y - origin.1) * factor.1,
            ),
            Self::Rotate { pivot, degrees } => {
                let (sin, cos) = degrees.to_radians().sin_cos();
                let (dx, dy) = (x - pivot.0, y - pivot.1);
                (pivot.0 + cos * dx - sin * dy, pivot.1 + sin * dx + cos * dy)
            }
            Self::Remove => (x, y),
        }
    }
}

impl ContentMove {
    pub fn placement(&self) -> Placement {
        Placement {
            offset: self.offset,
            scale: self.scale,
            rotation: self.rotation,
        }
    }

    /// Whether it changes the `nth` content of `kind` the base page draws
    /// at `rect` (not a copy of it).
    pub fn moves(&self, kind: ContentKind, rect: [f64; 4], nth: u32) -> bool {
        self.copy.is_none() && self.names_nth(kind, rect, nth)
    }

    /// Whether it is the change, or copy `copy`, of the `nth` content of
    /// `kind` the base page draws at `rect`.
    pub fn is(&self, kind: ContentKind, rect: [f64; 4], nth: u32, copy: Option<u32>) -> bool {
        self.copy == copy && self.names_nth(kind, rect, nth)
    }

    /// Whether it names the first content of `kind` the base page draws at
    /// `rect`.
    pub fn names(&self, kind: ContentKind, rect: [f64; 4]) -> bool {
        self.names_nth(kind, rect, 0)
    }

    /// Whether it names the `nth` content of `kind` the base page draws at
    /// `rect`.
    pub fn names_nth(&self, kind: ContentKind, rect: [f64; 4], nth: u32) -> bool {
        self.kind == kind
            && self.nth == nth
            && self
                .rect
                .iter()
                .zip(rect)
                .all(|(a, b)| (a - b).abs() < SAME_RECT)
    }

    /// Whether it leaves its content as it is.
    pub fn is_unchanged(&self) -> bool {
        !self.removed
            && self.offset.iter().all(|value| value.abs() < 1e-6)
            && self.scale.iter().all(|value| (value - 1.).abs() < 1e-9)
            && self.rotation == 0.
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.rect.iter().all(|value| value.is_finite())
            || self.rect[0] > self.rect[2]
            || self.rect[1] > self.rect[3]
        {
            return Err("moved content has invalid bounds".into());
        }
        if !self
            .offset
            .iter()
            .all(|value| value.is_finite() && value.abs() <= MAX_OFFSET)
        {
            return Err("moved content has an invalid offset".into());
        }
        if !self
            .scale
            .iter()
            .all(|value| value.is_finite() && (MIN_SCALE..=MAX_SCALE).contains(value))
            || (self.kind == ContentKind::Text && !is_unscaled(&self.scale))
        {
            return Err("resized content has an invalid size".into());
        }
        if !self.rotation.is_finite()
            || self.rotation.abs() > 180.
            || (self.kind == ContentKind::Text && self.rotation != 0.)
        {
            return Err("turned content has an invalid angle".into());
        }
        Ok(())
    }
}

/// A piece of content to change: what it is, where the base page draws it,
/// and how table edits and changes place it now; or a copy of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovedContent {
    pub kind: ContentKind,
    pub rect: [f64; 4],
    /// Which content at `rect` it is ([`ContentMove::nth`]).
    pub nth: u32,
    pub from: Placement,
    /// The copy it is, if it is one.
    pub copy: Option<u32>,
}

impl MovedContent {
    /// Whether it and `other` are the same piece (or the same copy).
    pub fn same(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.rect == other.rect
            && self.nth == other.nth
            && self.copy == other.copy
    }
}

impl MovedContent {
    /// Where it is shown now.
    pub fn shown(&self) -> [f64; 4] {
        self.from.shown(self.rect)
    }
}

/// How `item` is placed once `change` changes it from how it is shown: a
/// text line is never scaled, so a resize moves it with its corner.
pub fn changed_placement(item: &MovedContent, change: ContentChange) -> Placement {
    match (item.kind, change) {
        (ContentKind::Text, ContentChange::Resize { .. } | ContentChange::Rotate { .. }) => {
            let shown = item.shown();
            let (x, y) = change.map((shown[0], shown[1]));
            Placement {
                offset: [
                    item.from.offset[0] + x - shown[0],
                    item.from.offset[1] + y - shown[1],
                ],
                ..Placement::UNMOVED
            }
        }
        _ => item.from.changed(item.rect, change),
    }
}

/// The change of `moves` to the (first) content of `kind` the base page
/// draws at `rect`, when there is one.
pub fn change_of(moves: &[ContentMove], kind: ContentKind, rect: [f64; 4]) -> Option<&ContentMove> {
    change_of_nth(moves, kind, rect, 0)
}

/// The change of `moves` to the `nth` content of `kind` the base page draws
/// at `rect`, when there is one.
pub fn change_of_nth(
    moves: &[ContentMove],
    kind: ContentKind,
    rect: [f64; 4],
    nth: u32,
) -> Option<&ContentMove> {
    moves.iter().find(|item| item.moves(kind, rect, nth))
}

/// How far `moves` move the content of `kind` the base page draws at
/// `rect`, when one does and does not remove it.
pub fn moved_by(moves: &[ContentMove], kind: ContentKind, rect: [f64; 4]) -> Option<(f64, f64)> {
    change_of(moves, kind, rect)
        .filter(|item| !item.removed)
        .map(|item| (item.offset[0], item.offset[1]))
}

/// Where a page's table edits and moves put the text line at `rect` (its
/// core box): a changed line by its change alone, any other as its tables
/// move it. `None` when it is deleted, or a deleted row or column removes
/// it.
pub fn line_offset(
    tables: &[&PageTableEdit],
    moves: &[ContentMove],
    rect: [f64; 4],
) -> Option<(f64, f64)> {
    match change_of(moves, ContentKind::Text, rect) {
        Some(item) if item.removed => None,
        Some(item) => Some((item.offset[0], item.offset[1])),
        None => content_offset(tables, rect),
    }
}

/// How content of `kind` the base page draws at `rect` is placed: by its
/// change, or as tables move it. `None` when it is deleted.
pub fn placement_of(
    tables: &[&PageTableEdit],
    moves: &[ContentMove],
    kind: ContentKind,
    rect: [f64; 4],
) -> Option<Placement> {
    placement_of_nth(tables, moves, kind, rect, 0)
}

/// [`placement_of`] the `nth` content of `kind` at `rect`.
pub fn placement_of_nth(
    tables: &[&PageTableEdit],
    moves: &[ContentMove],
    kind: ContentKind,
    rect: [f64; 4],
    nth: u32,
) -> Option<Placement> {
    match change_of_nth(moves, kind, rect, nth) {
        Some(item) if item.removed => None,
        Some(item) => Some(item.placement()),
        None => content_offset(tables, rect).map(Placement::moved),
    }
}

/// A Form XObject a page draws (a CAD block) exploded in Edit PDF mode:
/// written into the page's own content where it was drawn, so what it
/// draws is page content to select, move and edit. It names the form as a
/// move names an image (`rect`, `nth`), and keeps how it was placed when
/// exploded, so what it draws stays where it was shown.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContentExplode {
    pub page_index: u32,
    pub rect: [f64; 4],
    #[serde(default, skip_serializing_if = "is_first")]
    pub nth: u32,
    #[serde(default, skip_serializing_if = "is_origin")]
    pub offset: [f64; 2],
    #[serde(default = "unscaled", skip_serializing_if = "is_unscaled")]
    pub scale: [f64; 2],
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
    /// Instead of a form of the page, content pasted from another page or
    /// PDF, written after the page's own: it names nothing, and what it
    /// draws is page content as what a form draws is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste: Option<ContentPaste>,
}

/// Content copied from a page (a clip: `pdf_text_edit::content_clip`),
/// pasted into another: where it goes, and the clip, named by its SHA-256.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentPaste {
    pub id: String,
    /// From the clip's page space to the page's.
    pub matrix: [f64; 6],
    /// The clip itself. The worker is sent it once, not with every edit.
    #[serde(skip)]
    pub pdf: Option<std::sync::Arc<Vec<u8>>>,
}

impl PartialEq for ContentPaste {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.matrix == other.matrix
    }
}

fn is_origin(offset: &[f64; 2]) -> bool {
    *offset == [0., 0.]
}

impl ContentExplode {
    /// Explodes the `nth` form the base page draws at `rect`, placed as
    /// `placement` places it.
    pub fn new(page_index: u32, rect: [f64; 4], nth: u32, placement: Placement) -> Self {
        Self {
            page_index,
            rect,
            nth,
            offset: placement.offset,
            scale: placement.scale,
            rotation: placement.rotation,
            paste: None,
        }
    }

    /// Content pasted into page `page_index`, whose bounds there are
    /// `rect`.
    pub fn pasted(page_index: u32, rect: [f64; 4], paste: ContentPaste) -> Self {
        Self {
            page_index,
            rect,
            nth: 0,
            offset: [0., 0.],
            scale: [1., 1.],
            rotation: 0.,
            paste: Some(paste),
        }
    }

    pub fn placement(&self) -> Placement {
        Placement {
            offset: self.offset,
            scale: self.scale,
            rotation: self.rotation,
        }
    }

    /// Whether it explodes the `nth` form at `rect`.
    pub fn names(&self, rect: [f64; 4], nth: u32) -> bool {
        self.paste.is_none()
            && self.nth == nth
            && self
                .rect
                .iter()
                .zip(rect)
                .all(|(a, b)| (a - b).abs() < SAME_RECT)
    }

    pub fn validate(&self) -> Result<(), String> {
        let item = ContentMove {
            page_index: self.page_index,
            kind: ContentKind::Image,
            rect: self.rect,
            nth: self.nth,
            offset: self.offset,
            scale: self.scale,
            rotation: self.rotation,
            removed: false,
            copy: None,
        };
        item.validate()
            .map_err(|error| format!("exploded block: {error}"))
    }
}

/// A drawing or image a page draws, which Edit PDF mode can select and
/// move.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PageObject {
    #[serde(default, skip_serializing_if = "is_drawing")]
    pub kind: ContentKind,
    /// Its bounds in page space, to a thousandth of a point (content is
    /// named to a hundredth).
    #[serde(serialize_with = "serialize_rect")]
    pub rect: [f64; 4],
    /// The text lines (their core boxes) a drawing's clip clips, which move
    /// with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<[f64; 4]>,
    /// A painted path's subpaths in page space (curves flattened), so it is
    /// picked where it is drawn rather than anywhere in its bounds, as a
    /// CAD program picks a line. Empty for images, clipped groups and paths
    /// past the page's budget, which are picked by their bounds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<Vec<[f64; 2]>>,
    /// How far its stroke reaches either side of the outline: half its
    /// line width, in page space; 0 when it is not stroked.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reach: f64,
    /// It is filled, so a point inside its outline picks it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub filled: bool,
    /// A path that is one straight segment across its bounds keeps no
    /// outline, as most of a drawing set's paths are: `Some(true)` runs
    /// from the bounds' bottom-left corner to their top-right, `Some(false)`
    /// from their top-left to their bottom-right.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment: Option<bool>,
    /// Which content of its kind at `rect` it is ([`ContentMove::nth`]).
    #[serde(default, skip_serializing_if = "is_first")]
    pub nth: u32,
    /// It is a Form XObject (a CAD block), which Explode writes into the
    /// page's own content.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub form: bool,
    /// The lettering it is a stroke or letter of, drawn as paths rather
    /// than text (AutoCAD's SHX fonts, text outlined by Bluebeam or Quartz),
    /// numbered from 1 on its page; 0 when it is not lettering. A click
    /// picks the lettering whole, Alt the piece alone.
    #[serde(default, skip_serializing_if = "is_first")]
    pub cluster: u32,
}

fn is_drawing(kind: &ContentKind) -> bool {
    *kind == ContentKind::Drawing
}

/// A drawing set's page has tens of thousands of objects: their bounds go
/// to the app rounded, which keeps the page's description a third smaller.
fn serialize_rect<S: serde::Serializer>(rect: &[f64; 4], serializer: S) -> Result<S::Ok, S::Error> {
    rect.map(|value| (value * 1000.).round() / 1000.)
        .serialize(serializer)
}

fn is_zero(value: &f64) -> bool {
    *value == 0.
}

impl PageObject {
    /// Its outline, with a straight segment's points spelled out.
    pub fn points(&self) -> Vec<Vec<[f64; 2]>> {
        let [left, bottom, right, top] = self.rect;
        match self.segment {
            Some(true) => vec![vec![[left, bottom], [right, top]]],
            Some(false) => vec![vec![[left, top], [right, bottom]]],
            None => self.outline.clone(),
        }
    }

    /// This object where `placement` shows it.
    pub fn placed(&self, placement: Placement) -> Self {
        let rect = self.rect;
        Self {
            rect: placement.shown(rect),
            segment: None,
            outline: self
                .points()
                .iter()
                .map(|subpath| {
                    subpath
                        .iter()
                        .map(|&[x, y]| {
                            let (x, y) = placement.map(rect, (x, y));
                            [x, y]
                        })
                        .collect()
                })
                .collect(),
            // A moved or resized path keeps its line width.
            ..self.clone()
        }
    }

    /// Whether `point` is on what it draws, within `tolerance`: on its
    /// stroke or in its fill, or, without an outline, in its bounds.
    pub fn hit(&self, point: (f64, f64), tolerance: f64) -> bool {
        self.distance(point, tolerance).is_some()
    }

    /// How far `point` is from what it draws, beyond its stroke's reach:
    /// 0 in its fill or, without an outline, in its bounds. `None` when it
    /// is further than `tolerance`.
    pub fn distance(&self, (x, y): (f64, f64), tolerance: f64) -> Option<f64> {
        let rect = self.rect;
        if x < rect[0] - tolerance - self.reach
            || x > rect[2] + tolerance + self.reach
            || y < rect[1] - tolerance - self.reach
            || y > rect[3] + tolerance + self.reach
        {
            return None;
        }
        let outline = self.points();
        if outline.is_empty() {
            return Some(0.);
        }
        let nearest = outline
            .iter()
            .map(|subpath| match subpath.as_slice() {
                [only] => (only[0] - x).hypot(only[1] - y),
                points => points
                    .windows(2)
                    .map(|pair| segment_distance((x, y), pair[0], pair[1]))
                    .fold(f64::INFINITY, f64::min),
            })
            .fold(f64::INFINITY, f64::min);
        let beyond = (nearest - self.reach).max(0.);
        // A fill closes each subpath and fills by the even-odd rule.
        let inside = self.filled
            && outline
                .iter()
                .filter(|subpath| subpath.len() > 2)
                .map(|subpath| {
                    let mut crossings = 0;
                    for (index, a) in subpath.iter().enumerate() {
                        let b = subpath[(index + 1) % subpath.len()];
                        if (a[1] > y) != (b[1] > y)
                            && x < a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
                        {
                            crossings += 1;
                        }
                    }
                    crossings
                })
                .sum::<usize>()
                % 2
                == 1;
        if inside {
            Some(0.)
        } else {
            (beyond <= tolerance).then_some(beyond)
        }
    }
}

/// How far `point` is from the segment `a`–`b`.
fn segment_distance((x, y): (f64, f64), a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length > 0. {
        (((x - a[0]) * dx + (y - a[1]) * dy) / length).clamp(0., 1.)
    } else {
        0.
    };
    (x - (a[0] + t * dx)).hypot(y - (a[1] + t * dy))
}

/// The share of the page's area a drawing may cover and still be picked: a
/// larger one is the page's background.
pub const MAX_OBJECT_SHARE: f64 = 0.8;

/// The object of `objects` at page `point`: of those within `tolerance`
/// ([`PageObject::distance`]), the nearest, as a CAD program picks; of
/// those about as near (within a quarter of `tolerance`), the smallest.
/// Objects covering most of `page_box` and those inside `excluded`
/// (tables) are not picked.
pub fn object_at(
    objects: &[PageObject],
    page_box: Option<[f64; 4]>,
    excluded: &[[f64; 4]],
    point: (f64, f64),
    tolerance: f64,
) -> Option<PageObject> {
    let area = |rect: [f64; 4]| (rect[2] - rect[0]).max(0.) * (rect[3] - rect[1]).max(0.);
    let page_area = page_box.map(area);
    let hits = objects
        .iter()
        .filter(|object| page_area.is_none_or(|page| area(object.rect) < MAX_OBJECT_SHARE * page))
        .filter(|object| {
            !excluded
                .iter()
                .any(|outer| contains(*outer, object.rect, 1.))
        })
        .filter_map(|object| Some((object.distance(point, tolerance)?, object)))
        .collect::<Vec<_>>();
    let nearest = hits
        .iter()
        .map(|(distance, _)| *distance)
        .fold(f64::INFINITY, f64::min);
    hits.into_iter()
        .filter(|(distance, _)| *distance <= nearest + tolerance / 4.)
        .min_by(|(_, a), (_, b)| area(a.rect).total_cmp(&area(b.rect)))
        .map(|(_, object)| object.clone())
}

/// Whether `outer`, grown by `tolerance`, holds all of `inner`.
pub fn contains(outer: [f64; 4], inner: [f64; 4], tolerance: f64) -> bool {
    inner[0] >= outer[0] - tolerance
        && inner[1] >= outer[1] - tolerance
        && inner[2] <= outer[2] + tolerance
        && inner[3] <= outer[3] + tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: ContentKind, rect: [f64; 4], offset: [f64; 2]) -> ContentMove {
        ContentMove {
            page_index: 0,
            kind,
            rect,
            nth: 0,
            offset,
            scale: [1., 1.],
            rotation: 0.,
            removed: false,
            copy: None,
        }
    }

    #[test]
    fn a_move_names_its_content_by_kind_and_bounds() {
        let moves = [item(ContentKind::Text, [10., 20., 50., 24.], [5., -3.])];
        assert_eq!(
            moved_by(&moves, ContentKind::Text, [10., 20., 50., 24.005]),
            Some((5., -3.))
        );
        assert_eq!(
            moved_by(&moves, ContentKind::Image, [10., 20., 50., 24.]),
            None
        );
        assert_eq!(
            moved_by(&moves, ContentKind::Text, [10., 21., 50., 24.]),
            None
        );
        assert!(item(ContentKind::Text, [0., 0., 1., 1.], [0., 0.]).is_unchanged());
        assert!(
            item(ContentKind::Text, [0., 0., 1., 1.], [f64::NAN, 0.])
                .validate()
                .is_err()
        );
        assert!(
            item(ContentKind::Text, [2., 0., 1., 1.], [0., 0.])
                .validate()
                .is_err()
        );
    }

    fn object(kind: ContentKind, rect: [f64; 4]) -> PageObject {
        PageObject {
            kind,
            rect,
            ..PageObject::default()
        }
    }

    #[test]
    fn the_smallest_object_under_a_point_is_picked() {
        let objects = [
            object(ContentKind::Drawing, [0., 0., 600., 800.]),
            object(ContentKind::Image, [100., 100., 300., 300.]),
            object(ContentKind::Drawing, [150., 150., 200., 200.]),
            object(ContentKind::Drawing, [400., 400., 500., 410.]),
        ];
        let page = Some([0., 0., 600., 800.]);
        let at = |point| object_at(&objects, page, &[[390., 390., 510., 420.]], point, 2.);
        assert_eq!(at((160., 160.)).unwrap().rect, objects[2].rect);
        assert_eq!(at((110., 110.)).unwrap().rect, objects[1].rect);
        // The page's background and a table's part are not picked.
        assert_eq!(at((50., 50.)), None);
        assert_eq!(at((450., 405.)), None);
    }

    #[test]
    fn a_path_is_picked_on_its_stroke_or_in_its_fill_not_anywhere_in_its_bounds() {
        // A room's walls, a column and a thin grid line, as a CAD drawing
        // draws them on a full sheet.
        let walls = PageObject {
            rect: [100., 100., 500., 400.],
            outline: vec![vec![
                [100., 100.],
                [500., 100.],
                [500., 400.],
                [100., 400.],
                [100., 100.],
            ]],
            reach: 1.5,
            ..PageObject::default()
        };
        let column = PageObject {
            rect: [92., 92., 108., 108.],
            outline: vec![vec![[92., 92.], [108., 92.], [108., 108.], [92., 108.]]],
            filled: true,
            ..PageObject::default()
        };
        let grid = PageObject {
            rect: [300., 50., 300., 450.],
            outline: vec![vec![[300., 50.], [300., 450.]]],
            reach: 0.125,
            ..PageObject::default()
        };
        let objects = [walls.clone(), column.clone(), grid.clone()];
        let page = Some([0., 0., 1190., 842.]);
        let at = |point| object_at(&objects, page, &[], point, 2.);
        // On a wall, beside it within the tolerance, and not inside the room.
        assert_eq!(at((250., 401.)), Some(walls.clone()));
        assert_eq!(at((502.5, 250.)), Some(walls.clone()));
        assert_eq!(at((200., 250.)), None);
        // Inside the column's fill, where it beats the walls under it.
        assert_eq!(at((101., 101.)), Some(column.clone()));
        // The grid line, picked inside the room where it is drawn.
        assert_eq!(at((301., 250.)), Some(grid.clone()));
        // Nearer a line than a small thing beside it, the line is picked:
        // a short stroke 1.8 points below the wall's top is in reach and
        // smaller, but further from a press on the wall; pressed on, it
        // wins over the wall about as near.
        let tick = PageObject {
            rect: [240., 398.2, 260., 398.2],
            segment: Some(true),
            reach: 0.1,
            ..PageObject::default()
        };
        let objects = [walls.clone(), tick.clone()];
        let at = |point| object_at(&objects, page, &[], point, 2.);
        assert_eq!(at((250., 400.)), Some(walls.clone()));
        assert!(tick.hit((250., 400.), 2.));
        assert_eq!(at((250., 398.2)), Some(tick));
        // Moved with its content, the outline goes with it.
        let moved = walls.placed(Placement::moved((50., 0.)));
        assert!(moved.hit((550., 250.), 1.));
        assert!(!moved.hit((500., 250.), 1.));
    }

    fn near(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn turned_content_turns_about_its_centre_and_resizes_without_skewing() {
        let rect = [100., 100., 140., 120.];
        // A quarter turn about its centre (120, 110), then 10 points right.
        let turned = Placement::moved((10., 0.)).changed(
            rect,
            ContentChange::Rotate {
                pivot: (130., 110.),
                degrees: 90.,
            },
        );
        assert_eq!(turned.rotation, 90.);
        assert!(near(turned.centre(rect), (130., 110.)));
        assert!(near(turned.map(rect, (100., 100.)), (140., 90.)));
        let shown = turned.shown(rect);
        for (a, b) in shown.iter().zip([120., 90., 140., 130.]) {
            assert!((a - b).abs() < 1e-9, "{shown:?}");
        }
        // Its matrix places it as its points do.
        let [a, b, c, d, e, f] = turned.matrix(rect);
        let (x, y) = (140., 120.);
        assert!(near(
            (a * x + c * y + e, b * x + d * y + f),
            turned.map(rect, (x, y))
        ));
        // Widened on the page, a quarter-turned piece grows taller in its
        // own axes, and its centre follows.
        let widened = turned.changed(
            rect,
            ContentChange::Resize {
                origin: (120., 90.),
                factor: (2., 1.),
            },
        );
        assert_eq!(widened.scale, [1., 2.]);
        let shown = widened.shown(rect);
        for (a, b) in shown.iter().zip([120., 90., 160., 130.]) {
            assert!((a - b).abs() < 1e-9, "{shown:?}");
        }
        // At another angle it keeps its proportions.
        let slanted = Placement::UNMOVED.changed(
            rect,
            ContentChange::Rotate {
                pivot: (120., 110.),
                degrees: 30.,
            },
        );
        let grown = slanted.changed(
            rect,
            ContentChange::Resize {
                origin: (0., 0.),
                factor: (1.5, 1.),
            },
        );
        assert_eq!(grown.scale, [1.5, 1.5]);
        assert_eq!(grown.rotation, 30.);
        // Angles stay within half a turn either way.
        assert_eq!(normalized_degrees(270.), -90.);
        assert_eq!(normalized_degrees(-180.), 180.);
        assert_eq!(normalized_degrees(360.0000001), 0.);
        // Text is never turned.
        let mut text = item(ContentKind::Text, rect, [0., 0.]);
        text.rotation = 15.;
        assert!(text.validate().is_err());
    }

    #[test]
    fn a_straight_segment_is_picked_along_its_diagonal() {
        let falling = PageObject {
            rect: [0., 0., 100., 100.],
            segment: Some(false),
            reach: 0.5,
            ..PageObject::default()
        };
        assert!(falling.hit((50., 50.), 1.));
        assert!(falling.hit((10., 90.), 1.));
        assert!(!falling.hit((10., 10.), 1.));
        // Placed, it is spelled out and turned with its content.
        let turned = falling.placed(Placement::UNMOVED.changed(
            falling.rect,
            ContentChange::Rotate {
                pivot: (50., 50.),
                degrees: 90.,
            },
        ));
        assert_eq!(turned.segment, None);
        assert!(turned.hit((10., 10.), 1.));
        assert!(!turned.hit((10., 90.), 1.));
    }
}
