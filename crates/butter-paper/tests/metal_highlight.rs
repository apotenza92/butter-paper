//! Actual Metal coverage and compositing regressions. These are separate from
//! GPUI's no-op test renderer and require an explicitly selected hardware run.
#![cfg(target_os = "macos")]

use gpui::{
    Bounds, DevicePixels, Path, ScaledPixels, Scene, point, px, size,
};

#[test]
#[ignore = "requires a real macOS Metal device; run explicitly with --ignored"]
fn opaque_multiply_gpu_pixels_cover_union_order_alpha_and_edges() {
    use gpui::{PlatformHeadlessRenderer, rgba};

    fn triangle(colour: u32, multiply: bool, copies: usize) -> Path<ScaledPixels> {
        let mut path = Path::new(point(px(4.25), px(4.25)));
        for _ in 0..copies {
            path.push_triangle(
                (
                    point(px(4.25), px(4.25)),
                    point(px(28.25), px(4.25)),
                    point(px(4.25), px(28.25)),
                ),
                (point(0., 1.), point(0., 1.), point(0., 1.)),
            );
        }
        path.content_mask.bounds = Bounds::new(point(px(0.), px(0.)), size(px(32.), px(32.)));
        path.color = rgba(colour).into();
        if multiply {
            path = path.with_multiply_over_opaque();
        }
        path.scale(1.)
    }

    fn backdrop(colour: u32) -> Path<ScaledPixels> {
        let mut path = Path::new(point(px(0.), px(0.)));
        path.line_to(point(px(32.), px(0.)));
        path.line_to(point(px(32.), px(32.)));
        path.line_to(point(px(0.), px(32.)));
        path.content_mask.bounds = Bounds::new(point(px(0.), px(0.)), size(px(32.), px(32.)));
        path.color = rgba(colour).into();
        path.scale(1.)
    }

    fn render(
        renderer: &mut dyn PlatformHeadlessRenderer,
        paths: Vec<Path<ScaledPixels>>,
    ) -> image::RgbaImage {
        let mut scene = Scene::default();
        scene.insert_primitive(backdrop(0x3366ccff));
        for path in paths {
            scene.insert_primitive(path);
        }
        scene.finish();
        renderer
            .render_scene_to_image(&scene, size(DevicePixels(32), DevicePixels(32)))
            .unwrap()
    }

    fn assert_pixel(image: &image::RgbaImage, expected: [u8; 4]) {
        let actual = image.get_pixel(8, 8).0;
        assert!(
            actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2),
            "{actual:?} != {expected:?}"
        );
    }

    let mut renderer = gpui_platform::current_headless_renderer()
        .expect("the macOS test must have a Metal renderer");
    let single = render(renderer.as_mut(), vec![triangle(0xffff0080, true, 1)]);
    assert_pixel(&single, [51, 102, 102, 255]);
    let mut clipped_path = triangle(0xffff0080, true, 1);
    clipped_path.content_mask.bounds = Bounds::new(
        point(gpui::ScaledPixels(8.), gpui::ScaledPixels(8.)),
        size(gpui::ScaledPixels(16.), gpui::ScaledPixels(16.)),
    );
    let clipped = render(renderer.as_mut(), vec![clipped_path]);
    assert_pixel(&clipped, [51, 102, 102, 255]);
    let mut clipped_partial_edges = 0;
    for (x, y, pixel) in clipped.enumerate_pixels() {
        if !(8..24).contains(&x) || !(8..24).contains(&y) {
            assert_eq!(
                pixel.0,
                [51, 102, 204, 255],
                "content mask leaked at {x},{y}"
            );
        } else {
            assert_eq!(
                pixel,
                single.get_pixel(x, y),
                "mask changed interior coverage at {x},{y}"
            );
            clipped_partial_edges += usize::from(pixel[2] > 105 && pixel[2] < 200);
        }
    }
    assert!(
        clipped_partial_edges > 0,
        "clipped fixture must retain antialiased path-edge coverage"
    );
    let self_overlap = render(renderer.as_mut(), vec![triangle(0xffff0080, true, 3)]);
    assert_eq!(
        single.as_raw(),
        self_overlap.as_raw(),
        "one path must union its own coverage, including antialiased edges"
    );
    let separate = render(
        renderer.as_mut(),
        vec![triangle(0xffff0080, true, 1), triangle(0xffff0080, true, 1)],
    );
    assert_pixel(&separate, [51, 102, 51, 255]);
    assert_pixel(
        &render(renderer.as_mut(), vec![triangle(0xffff0000, true, 1)]),
        [51, 102, 204, 255],
    );
    assert_pixel(
        &render(renderer.as_mut(), vec![triangle(0xffff00ff, true, 1)]),
        [51, 102, 0, 255],
    );
    assert_pixel(
        &render(
            renderer.as_mut(),
            vec![
                triangle(0xffff0080, true, 1),
                triangle(0xff0000ff, false, 1),
            ],
        ),
        [255, 0, 0, 255],
    );
    assert_pixel(
        &render(
            renderer.as_mut(),
            vec![
                triangle(0xff0000ff, false, 1),
                triangle(0x00ffff80, true, 1),
            ],
        ),
        [127, 0, 0, 255],
    );
    assert_eq!(single.get_pixel(0, 0).0, [51, 102, 204, 255]);
    let edges: Vec<_> = single
        .pixels()
        .filter(|pixel| pixel[2] > 105 && pixel[2] < 200)
        .collect();
    assert!(
        !edges.is_empty(),
        "fixture must exercise fractional edge coverage"
    );
    for pixel in edges {
        assert_eq!(pixel[0], 51);
        assert_eq!(pixel[1], 102);
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn opaque_multiply_flag_survives_scaling_and_scene_insertion() {
    let mut path = Path::new(point(px(2.), px(3.))).with_multiply_over_opaque();
    path.line_to(point(px(12.), px(3.)));
    path.line_to(point(px(12.), px(13.)));
    path.content_mask.bounds = Bounds::new(point(px(0.), px(0.)), size(px(20.), px(20.)));
    let scaled = path.scale(2.);
    assert!(scaled.multiplies_over_opaque());
    assert_eq!(scaled.bounds, path.bounds.scale(2.));
    let mut scene = Scene::default();
    scene.insert_primitive(scaled);
    assert_eq!(scene.paths.len(), 1);
    assert!(scene.paths[0].multiplies_over_opaque());
    assert!(
        !Path::new(point(px(0.), px(0.)))
            .scale(2.)
            .multiplies_over_opaque()
    );
}

#[test]
#[should_panic(expected = "opaque multiply paths require a solid background")]
fn opaque_multiply_rejects_non_solid_background() {
    let mut path = Path::new(point(px(0.), px(0.))).with_multiply_over_opaque();
    path.color = gpui::linear_gradient(
        0.,
        gpui::linear_color_stop(gpui::rgb(0xff0000), 0.),
        gpui::linear_color_stop(gpui::rgb(0x0000ff), 1.),
    );
    path.scale(2.).multiplies_over_opaque();
}
