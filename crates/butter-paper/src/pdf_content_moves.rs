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
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    /// A text line.
    Text,
    /// A painted path.
    Drawing,
    /// An image or form drawn with `Do`.
    Image,
}

/// A piece of page content and how it changes: scaled from its bottom-left
/// corner, then moved, or removed.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContentMove {
    pub page_index: u32,
    pub kind: ContentKind,
    /// Where the base page draws it (left, bottom, right, top): a text
    /// line's core box ([`crate::pdf_tables::line_core_box`]), a path's
    /// bounds or an image's.
    pub rect: [f64; 4],
    /// How far it moves, in page space.
    pub offset: [f64; 2],
    /// How much wider and taller it is drawn; text is never scaled.
    #[serde(default = "unscaled", skip_serializing_if = "is_unscaled")]
    pub scale: [f64; 2],
    /// It is deleted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
    /// It is not the content itself but copy number `n` of it, drawn as
    /// well, placed by `offset` and `scale` from where the base page draws
    /// the content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<u32>,
}

fn unscaled() -> [f64; 2] {
    [1., 1.]
}

fn is_unscaled(scale: &[f64; 2]) -> bool {
    *scale == [1., 1.]
}

/// How content is placed: scaled by `scale` from the bottom-left corner of
/// the bounds the page draws it at, then moved by `offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub offset: [f64; 2],
    pub scale: [f64; 2],
}

impl Placement {
    pub const UNMOVED: Self = Self {
        offset: [0., 0.],
        scale: [1., 1.],
    };

    pub fn moved(offset: (f64, f64)) -> Self {
        Self {
            offset: [offset.0, offset.1],
            scale: [1., 1.],
        }
    }

    /// Where page point `point` of content the page draws at `rect` goes.
    pub fn map(&self, rect: [f64; 4], (x, y): (f64, f64)) -> (f64, f64) {
        (
            rect[0] + (x - rect[0]) * self.scale[0] + self.offset[0],
            rect[1] + (y - rect[1]) * self.scale[1] + self.offset[1],
        )
    }

    /// Where content the page draws at `rect` is shown.
    pub fn shown(&self, rect: [f64; 4]) -> [f64; 4] {
        let (x0, y0) = self.map(rect, (rect[0], rect[1]));
        let (x1, y1) = self.map(rect, (rect[2], rect[3]));
        [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]
    }

    /// As a page-space matrix (`[a b c d e f]`, row vectors).
    pub fn matrix(&self, rect: [f64; 4]) -> [f64; 6] {
        [
            self.scale[0],
            0.,
            0.,
            self.scale[1],
            rect[0] * (1. - self.scale[0]) + self.offset[0],
            rect[1] * (1. - self.scale[1]) + self.offset[1],
        ]
    }

    /// This placement changed further by `change`: moved, or resized about
    /// a page point, as the content is shown now.
    pub fn changed(&self, rect: [f64; 4], change: ContentChange) -> Self {
        match change {
            ContentChange::Move(dx, dy) => Self {
                offset: [self.offset[0] + dx, self.offset[1] + dy],
                scale: self.scale,
            },
            ContentChange::Resize { origin, factor } => Self {
                scale: [self.scale[0] * factor.0, self.scale[1] * factor.1],
                offset: [
                    origin.0 + (rect[0] + self.offset[0] - origin.0) * factor.0 - rect[0],
                    origin.1 + (rect[1] + self.offset[1] - origin.1) * factor.1 - rect[1],
                ],
            },
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
    /// Deleted.
    Remove,
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
            Self::Remove => (x, y),
        }
    }
}

impl ContentMove {
    pub fn placement(&self) -> Placement {
        Placement {
            offset: self.offset,
            scale: self.scale,
        }
    }

    /// Whether it changes the content of `kind` the base page draws at
    /// `rect` (not a copy of it).
    pub fn moves(&self, kind: ContentKind, rect: [f64; 4]) -> bool {
        self.copy.is_none() && self.names(kind, rect)
    }

    /// Whether it is the change, or copy `copy`, of the content of `kind`
    /// the base page draws at `rect`.
    pub fn is(&self, kind: ContentKind, rect: [f64; 4], copy: Option<u32>) -> bool {
        self.copy == copy && self.names(kind, rect)
    }

    /// Whether it names the content of `kind` the base page draws at `rect`.
    pub fn names(&self, kind: ContentKind, rect: [f64; 4]) -> bool {
        self.kind == kind
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
        Ok(())
    }
}

/// A piece of content to change: what it is, where the base page draws it,
/// and how table edits and changes place it now; or a copy of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovedContent {
    pub kind: ContentKind,
    pub rect: [f64; 4],
    pub from: Placement,
    /// The copy it is, if it is one.
    pub copy: Option<u32>,
}

impl MovedContent {
    /// Whether it and `other` are the same piece (or the same copy).
    pub fn same(&self, other: &Self) -> bool {
        self.kind == other.kind && self.rect == other.rect && self.copy == other.copy
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
        (ContentKind::Text, ContentChange::Resize { .. }) => {
            let shown = item.shown();
            let (x, y) = change.map((shown[0], shown[1]));
            Placement {
                offset: [
                    item.from.offset[0] + x - shown[0],
                    item.from.offset[1] + y - shown[1],
                ],
                scale: [1., 1.],
            }
        }
        _ => item.from.changed(item.rect, change),
    }
}

/// The change of `moves` to the content of `kind` the base page draws at
/// `rect`, when there is one.
pub fn change_of(moves: &[ContentMove], kind: ContentKind, rect: [f64; 4]) -> Option<&ContentMove> {
    moves.iter().find(|item| item.moves(kind, rect))
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
    match change_of(moves, kind, rect) {
        Some(item) if item.removed => None,
        Some(item) => Some(item.placement()),
        None => content_offset(tables, rect).map(Placement::moved),
    }
}

/// A drawing or image a page draws, which Edit PDF mode can select and
/// move.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageObject {
    pub kind: ContentKind,
    /// Its bounds in page space.
    pub rect: [f64; 4],
    /// The text lines (their core boxes) a drawing's clip clips, which move
    /// with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<[f64; 4]>,
}

/// The share of the page's area a drawing may cover and still be picked: a
/// larger one is the page's background.
pub const MAX_OBJECT_SHARE: f64 = 0.8;

/// The object of `objects` at page `point`: the smallest whose bounds,
/// grown by `tolerance`, hold it. Objects covering most of `page_box` and
/// those inside `excluded` (tables) are not picked.
pub fn object_at(
    objects: &[PageObject],
    page_box: Option<[f64; 4]>,
    excluded: &[[f64; 4]],
    point: (f64, f64),
    tolerance: f64,
) -> Option<PageObject> {
    let area = |rect: [f64; 4]| (rect[2] - rect[0]).max(0.) * (rect[3] - rect[1]).max(0.);
    let page_area = page_box.map(area);
    objects
        .iter()
        .filter(|object| {
            let rect = object.rect;
            point.0 >= rect[0] - tolerance
                && point.0 <= rect[2] + tolerance
                && point.1 >= rect[1] - tolerance
                && point.1 <= rect[3] + tolerance
        })
        .filter(|object| page_area.is_none_or(|page| area(object.rect) < MAX_OBJECT_SHARE * page))
        .filter(|object| {
            !excluded
                .iter()
                .any(|outer| contains(*outer, object.rect, 1.))
        })
        .min_by(|a, b| area(a.rect).total_cmp(&area(b.rect)))
        .cloned()
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
            offset,
            scale: [1., 1.],
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
            texts: Vec::new(),
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
}
