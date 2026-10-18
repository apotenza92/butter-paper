//! Butter Paper's own canvas cursors: resize arrows at a box's exact angle,
//! the rotate arrow and, on Windows, the move arrow. Standard cursors (arrow, hands, crosshair, text) stay the
//! system's, which follow the user's pointer size and colour settings.
//!
//! The cursors are drawn here from signed distance fields, black with a white
//! outline like the system cursors, and registered with GPUI once each.
//! Platforms that cannot show application cursors use each one's fallback.

use std::{
    collections::HashMap,
    f32::consts::{PI, TAU},
    sync::{Mutex, OnceLock},
};

use gpui::{CursorImage, CursorImageId, CursorRaster, CursorStyle, point, px, size};

/// Logical cursor size; the hot spot is its centre.
const CURSOR_SIZE: f32 = 24.;
const SCALES: [f32; 2] = [1., 2.];
/// Resize arrows are drawn in steps of this many degrees.
const ANGLE_STEP_DEGREES: f32 = 5.;
const OUTLINE: f32 = 1.25;

/// A double-headed resize arrow along `degrees`, measured clockwise from the
/// positive x axis on screen. Opposite directions share a cursor.
pub fn resize_cursor(degrees: f64) -> CursorStyle {
    let step = resize_step(degrees);
    let angle = f32::from(step) * ANGLE_STEP_DEGREES;
    CursorStyle::Image(registered(Shape::Resize(step), || {
        cursor_image(
            |x, y| resize_arrow_distance(x, y, angle.to_radians()),
            nearest_standard_resize(angle),
        )
    }))
}

/// The cursor over a rotation knob.
pub fn rotate_cursor() -> CursorStyle {
    CursorStyle::Image(registered(Shape::Rotate, || {
        cursor_image(rotate_arrow_distance, CursorStyle::PointingHand)
    }))
}

/// The cursor over a movable markup (`moving` false) and while moving it.
/// macOS and Linux show the system's open or closed hand. Windows has no hand
/// cursors, so it shows the four-headed move arrow Windows applications use.
pub fn move_cursor(moving: bool) -> CursorStyle {
    if cfg!(target_os = "windows") {
        CursorStyle::Image(registered(Shape::Move, || {
            cursor_image(move_arrow_distance, CursorStyle::OpenHand)
        }))
    } else if moving {
        CursorStyle::ClosedHand
    } else {
        CursorStyle::OpenHand
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Shape {
    Resize(u16),
    Rotate,
    Move,
}

fn resize_step(degrees: f64) -> u16 {
    let steps = (180. / ANGLE_STEP_DEGREES) as u16;
    let degrees = if degrees.is_finite() { degrees } else { 0. };
    ((degrees.rem_euclid(180.) / f64::from(ANGLE_STEP_DEGREES)).round() as u16) % steps
}

fn registered(shape: Shape, draw: impl FnOnce() -> CursorImage) -> CursorImageId {
    static CURSORS: OnceLock<Mutex<HashMap<Shape, CursorImageId>>> = OnceLock::new();
    let mut cursors = CURSORS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *cursors
        .entry(shape)
        .or_insert_with(|| gpui::register_cursor_image(draw()))
}

/// The nearest system resize cursor, for platforms without application cursors.
fn nearest_standard_resize(degrees: f32) -> CursorStyle {
    match ((degrees.rem_euclid(180.) / 45.).round() as i32).rem_euclid(4) {
        0 => CursorStyle::ResizeLeftRight,
        1 => CursorStyle::ResizeUpLeftDownRight,
        2 => CursorStyle::ResizeUpDown,
        _ => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn cursor_image(distance: impl Fn(f32, f32) -> f32, fallback: CursorStyle) -> CursorImage {
    let centre = CURSOR_SIZE / 2.;
    CursorImage {
        size: size(px(CURSOR_SIZE), px(CURSOR_SIZE)),
        hot_spot: point(px(centre), px(centre)),
        rasters: SCALES
            .iter()
            .map(|&scale| rasterize(&distance, scale))
            .collect(),
        fallback,
    }
}

/// Renders a shape given by its signed distance in logical pixels from the
/// cursor centre (negative inside): black fill inside a white outline.
fn rasterize(distance: &impl Fn(f32, f32) -> f32, scale: f32) -> CursorRaster {
    let pixels = (CURSOR_SIZE * scale).round() as u32;
    let centre = CURSOR_SIZE / 2.;
    let mut rgba = Vec::with_capacity((pixels * pixels * 4) as usize);
    for row in 0..pixels {
        for column in 0..pixels {
            let x = (column as f32 + 0.5) / scale - centre;
            let y = (row as f32 + 0.5) / scale - centre;
            let d = distance(x, y);
            // Coverage over about one device pixel of edge.
            let coverage = |edge: f32| (0.5 - edge * scale).clamp(0., 1.);
            let fill = coverage(d);
            let alpha = coverage(d - OUTLINE);
            let white = if alpha > 0. { 1. - fill / alpha } else { 0. };
            let channel = (white * 255.).round() as u8;
            rgba.extend_from_slice(&[channel, channel, channel, (alpha * 255.).round() as u8]);
        }
    }
    CursorRaster {
        scale,
        width: pixels,
        height: pixels,
        rgba,
    }
}

fn segment_distance(x: f32, y: f32, (ax, ay): (f32, f32), (bx, by): (f32, f32)) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
    (x - ax - t * dx).hypot(y - ay - t * dy)
}

/// Signed distance to a triangle, negative inside.
fn triangle_distance(x: f32, y: f32, corners: [(f32, f32); 3]) -> f32 {
    let edge = |index: usize| (corners[index], corners[(index + 1) % 3]);
    let outside = (0..3)
        .map(|index| {
            let ((ax, ay), (bx, by)) = edge(index);
            (bx - ax) * (y - ay) - (by - ay) * (x - ax)
        })
        .collect::<Vec<_>>();
    let inside = outside.iter().all(|side| *side >= 0.) || outside.iter().all(|side| *side <= 0.);
    let nearest = (0..3)
        .map(|index| {
            let (a, b) = edge(index);
            segment_distance(x, y, a, b)
        })
        .fold(f32::INFINITY, f32::min);
    if inside { -nearest } else { nearest }
}

/// An arrowhead with its tip at `tip`, pointing along `(dx, dy)`.
fn arrowhead(x: f32, y: f32, tip: (f32, f32), (dx, dy): (f32, f32)) -> f32 {
    const LENGTH: f32 = 4.5;
    const HALF_WIDTH: f32 = 4.;
    let base = (tip.0 - dx * LENGTH, tip.1 - dy * LENGTH);
    let (nx, ny) = (-dy * HALF_WIDTH, dx * HALF_WIDTH);
    triangle_distance(
        x,
        y,
        [tip, (base.0 + nx, base.1 + ny), (base.0 - nx, base.1 - ny)],
    )
}

fn resize_arrow_distance(x: f32, y: f32, angle: f32) -> f32 {
    const REACH: f32 = 8.5;
    const SHAFT_HALF_WIDTH: f32 = 1.;
    let (dy, dx) = angle.sin_cos();
    let shaft = segment_distance(
        x,
        y,
        (-dx * (REACH - 3.), -dy * (REACH - 3.)),
        (dx * (REACH - 3.), dy * (REACH - 3.)),
    ) - SHAFT_HALF_WIDTH;
    let forward = arrowhead(x, y, (dx * REACH, dy * REACH), (dx, dy));
    let backward = arrowhead(x, y, (-dx * REACH, -dy * REACH), (-dx, -dy));
    shaft.min(forward).min(backward)
}

/// Horizontal and vertical double-headed arrows, crossed.
fn move_arrow_distance(x: f32, y: f32) -> f32 {
    resize_arrow_distance(x, y, 0.).min(resize_arrow_distance(x, y, PI / 2.))
}

/// Three quarters of a circle, clockwise, ending in an arrowhead.
fn rotate_arrow_distance(x: f32, y: f32) -> f32 {
    const RADIUS: f32 = 6.;
    const HALF_WIDTH: f32 = 1.;
    // The arc runs clockwise on screen from `START` to `END`.
    const START: f32 = PI * 0.75;
    const END: f32 = PI * 0.75 + TAU * 0.72;
    let angle = y.atan2(x);
    let along = (angle - START).rem_euclid(TAU);
    let arc = if along <= END - START {
        (x.hypot(y) - RADIUS).abs()
    } else {
        let endpoint = |theta: f32| (RADIUS * theta.cos(), RADIUS * theta.sin());
        let (sx, sy) = endpoint(START);
        let (ex, ey) = endpoint(END);
        (x - sx).hypot(y - sy).min((x - ex).hypot(y - ey))
    } - HALF_WIDTH;
    // The head sits past the arc's end, pointing along it.
    let end = END + 0.35;
    let tip = (RADIUS * end.cos(), RADIUS * end.sin());
    let direction = (-end.sin(), end.cos());
    let head = arrowhead(
        x,
        y,
        (tip.0 + direction.0 * 2., tip.1 + direction.1 * 2.),
        direction,
    );
    arc.min(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(raster: &CursorRaster, x: f32, y: f32) -> u8 {
        let column = ((x + CURSOR_SIZE / 2.) * raster.scale) as u32;
        let row = ((y + CURSOR_SIZE / 2.) * raster.scale) as u32;
        raster.rgba[((row * raster.width + column) * 4 + 3) as usize]
    }

    #[test]
    fn opposite_directions_share_a_resize_cursor_in_five_degree_steps() {
        assert_eq!(resize_cursor(0.), resize_cursor(180.));
        assert_eq!(resize_cursor(-45.), resize_cursor(135.));
        assert_eq!(resize_cursor(31.), resize_cursor(30.));
        assert_ne!(resize_cursor(30.), resize_cursor(35.));
        assert_eq!(resize_step(f64::NAN), 0);
        assert_eq!(resize_step(178.), 0);
    }

    #[test]
    fn resize_cursors_fall_back_to_the_nearest_system_resize_cursor() {
        let fallback = |degrees| resize_cursor(degrees).standard();
        assert_eq!(fallback(0.), CursorStyle::ResizeLeftRight);
        assert_eq!(fallback(40.), CursorStyle::ResizeUpLeftDownRight);
        assert_eq!(fallback(90.), CursorStyle::ResizeUpDown);
        assert_eq!(fallback(-50.), CursorStyle::ResizeUpRightDownLeft);
        assert_eq!(rotate_cursor().standard(), CursorStyle::PointingHand);
    }

    #[test]
    fn resize_arrow_is_drawn_along_its_angle_with_an_outline() {
        let CursorStyle::Image(id) = resize_cursor(90.) else {
            panic!("resize cursors are application images");
        };
        let image = gpui::cursor_image(id).unwrap();
        assert_eq!(image.hot_spot, point(px(12.), px(12.)));
        let raster = image.raster_for_scale(2.).unwrap();
        assert_eq!((raster.width, raster.height), (48, 48));
        // Vertical: ink above and below the centre, none to the sides.
        assert_eq!(alpha_at(raster, 0., -7.), 255);
        assert_eq!(alpha_at(raster, 0., 7.), 255);
        assert_eq!(alpha_at(raster, 7., 0.), 0);
        let centre = ((24 * raster.width + 24) * 4) as usize;
        assert_eq!(
            &raster.rgba[centre..centre + 4],
            &[0, 0, 0, 255],
            "black shaft"
        );
        let edge = ((24 * raster.width + 24 + 3) * 4) as usize;
        assert!(
            raster.rgba[edge] > 200 && raster.rgba[edge + 3] > 200,
            "white outline"
        );
    }

    #[test]
    fn rotate_arrow_is_an_open_ring() {
        let CursorStyle::Image(id) = rotate_cursor() else {
            panic!("the rotate cursor is an application image");
        };
        let image = gpui::cursor_image(id).unwrap();
        let raster = image.raster_for_scale(1.).unwrap();
        assert_eq!(alpha_at(raster, 0., 0.), 0, "the centre is open");
        assert_eq!(
            alpha_at(raster, 0., -6.),
            255,
            "the ring passes above the centre"
        );
    }

    #[test]
    fn move_cursor_is_the_platform_move_cursor() {
        if cfg!(target_os = "windows") {
            let CursorStyle::Image(id) = move_cursor(false) else {
                panic!("Windows draws its move arrow");
            };
            assert_eq!(move_cursor(true), move_cursor(false));
            assert_eq!(move_cursor(false).standard(), CursorStyle::OpenHand);
            let raster = gpui::cursor_image(id).unwrap();
            let raster = raster.raster_for_scale(1.).unwrap();
            for (x, y) in [(7., 0.), (-7., 0.), (0., 7.), (0., -7.)] {
                assert_eq!(alpha_at(raster, x, y), 255, "an arm reaches ({x}, {y})");
            }
            assert_eq!(alpha_at(raster, 7., 7.), 0, "the quadrants are open");
        } else {
            assert_eq!(move_cursor(false), CursorStyle::OpenHand);
            assert_eq!(move_cursor(true), CursorStyle::ClosedHand);
        }
    }

    /// Writes each cursor at 2x, on a mid-grey card, for a visual check.
    #[test]
    #[ignore = "writes cursor previews for manual review"]
    fn write_cursor_previews_for_manual_review() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-scratch/canvas-cursors");
        std::fs::create_dir_all(&directory).unwrap();
        let cursors = [0., 30., 45., 90., 135.]
            .map(|degrees| (format!("resize-{degrees}"), resize_cursor(degrees)))
            .into_iter()
            .chain([
                ("rotate".to_owned(), rotate_cursor()),
                (
                    "move".to_owned(),
                    CursorStyle::Image(registered(Shape::Move, || {
                        cursor_image(move_arrow_distance, CursorStyle::OpenHand)
                    })),
                ),
            ]);
        for (name, cursor) in cursors {
            let CursorStyle::Image(id) = cursor else {
                unreachable!()
            };
            let image = gpui::cursor_image(id).unwrap();
            let raster = image.raster_for_scale(2.).unwrap();
            let mut card = image::RgbaImage::from_pixel(
                raster.width,
                raster.height,
                image::Rgba([128, 128, 128, 255]),
            );
            for (pixel, source) in card.pixels_mut().zip(raster.rgba.chunks_exact(4)) {
                let alpha = f32::from(source[3]) / 255.;
                for (target, value) in pixel.0.iter_mut().zip(source).take(3) {
                    *target = (f32::from(*value) * alpha + f32::from(*target) * (1. - alpha))
                        .round() as u8;
                }
            }
            card.save(directory.join(format!("{name}.png"))).unwrap();
        }
    }
}
