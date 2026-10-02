//! Development-only cost probe for ordered Highlight rendering on macOS.
//!
//! This deliberately stays outside the frozen Electron/GPUI comparison
//! protocol. It measures construction of the current GPUI path shape and an
//! offscreen Metal render that waits for GPU completion and reads the target
//! back. It does not measure a window, presentation, scanout, input latency,
//! process CPU/GPU utilisation, memory, idle use, or lifecycle recovery.
#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use gpui::{
    Bounds, DevicePixels, LineCap, LineJoin, Path, PathBuilder, PathStyle, ScaledPixels, Scene,
    StrokeOptions, point, px, rgba, size,
};
use serde_json::json;
use sha2::{Digest, Sha256};

const WIDTH: i32 = 1024;
const HEIGHT: i32 = 768;
const POINTS_PER_HIGHLIGHT: usize = 64;
const REPEATS: usize = 3;
const HIGHLIGHT_COUNTS: [usize; 4] = [1, 32, 128, 512];

fn page_mask() -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(0.), ScaledPixels(0.)),
        size(ScaledPixels(WIDTH as f32), ScaledPixels(HEIGHT as f32)),
    )
}

fn page_background() -> Path<ScaledPixels> {
    let mut builder = PathBuilder::fill();
    builder.add_polygon(
        &[
            point(px(0.), px(0.)),
            point(px(WIDTH as f32), px(0.)),
            point(px(WIDTH as f32), px(HEIGHT as f32)),
            point(px(0.), px(HEIGHT as f32)),
        ],
        true,
    );
    let mut path = builder.build().expect("page background must tessellate");
    path.color = rgba(0xffffffff).into();
    let mut path = path.scale(1.);
    path.content_mask.bounds = page_mask();
    path
}

fn representative_highlight(index: usize) -> Path<ScaledPixels> {
    // The 24-line band makes larger counts exercise both distinct strokes and
    // ordered overlap. Sixty-four samples and the canonical 12 pt, yellow,
    // fully opaque style match the maintained comparison workload contract.
    let band = index % 24;
    let pass = index / 24;
    let base_y = 82. + band as f32 * 25.;
    let mut builder = PathBuilder::stroke(px(12.)).with_style(PathStyle::Stroke(
        StrokeOptions::default()
            .with_line_width(12.)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round),
    ));
    for sample in 0..POINTS_PER_HIGHLIGHT {
        let progress = sample as f32 / (POINTS_PER_HIGHLIGHT - 1) as f32;
        let x = 64. + progress * (WIDTH as f32 - 128.);
        let wave = (progress * std::f32::consts::TAU * 2. + band as f32 * 0.17).sin();
        let y = base_y + wave * 3. + (pass % 3) as f32 * 0.2;
        if sample == 0 {
            builder.move_to(point(px(x), px(y)));
        } else {
            builder.line_to(point(px(x), px(y)));
        }
    }
    let mut path = builder
        .build()
        .expect("representative Highlight must tessellate")
        .with_multiply_over_opaque();
    path.color = rgba(0xffff00ff).into();
    let mut path = path.scale(1.);
    path.content_mask.bounds = page_mask();
    path
}

fn build_scene(count: usize) -> Scene {
    let mut scene = Scene::default();
    scene.insert_primitive(page_background());
    for index in 0..count {
        scene.insert_primitive(representative_highlight(index));
    }
    scene.finish();
    scene
}

fn elapsed_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn median(mut values: Vec<u64>) -> u64 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
#[ignore = "development evidence: requires a real macOS Metal device and is not a release budget"]
fn reports_many_highlight_cpu_preparation_and_metal_completion_cost() {
    let mut renderer = gpui_platform::current_headless_renderer()
        .expect("the macOS probe must have a real Metal headless renderer");
    let render_size = size(DevicePixels(WIDTH), DevicePixels(HEIGHT));

    // Initialise Metal pipelines and textures outside all reported intervals.
    renderer
        .render_scene_to_image(&build_scene(1), render_size)
        .expect("Metal warm-up must complete");

    let mut samples = Vec::new();
    for count in HIGHLIGHT_COUNTS {
        let mut cpu_prepare_ns = Vec::with_capacity(REPEATS);
        let mut scene = None;
        for _ in 0..REPEATS {
            let started = Instant::now();
            let prepared = build_scene(count);
            cpu_prepare_ns.push(elapsed_ns(started.elapsed()));
            scene = Some(prepared);
        }
        let scene = scene.expect("at least one preparation repeat");
        assert_eq!(scene.paths.len(), count + 1);
        assert!(!scene.paths[0].multiplies_over_opaque());
        assert!(scene.paths[1..].iter().all(Path::multiplies_over_opaque));
        assert!(
            scene.paths[1..]
                .iter()
                .all(|path| !path.vertices.is_empty())
        );

        let path_vertex_count: usize = scene.paths.iter().map(|path| path.vertices.len()).sum();
        assert_eq!(path_vertex_count % 3, 0, "all paths must be triangle lists");

        let mut metal_complete_and_readback_ns = Vec::with_capacity(REPEATS);
        let mut first_pixels = None;
        for _ in 0..REPEATS {
            let started = Instant::now();
            let image = renderer
                .render_scene_to_image(&scene, render_size)
                .expect("offscreen Metal render must complete");
            metal_complete_and_readback_ns.push(elapsed_ns(started.elapsed()));
            assert_eq!(image.dimensions(), (WIDTH as u32, HEIGHT as u32));
            assert_eq!(image.get_pixel(8, 8).0, [255, 255, 255, 255]);
            let highlighted = image.get_pixel(WIDTH as u32 / 2, 82).0;
            assert!(
                highlighted[0] >= 253
                    && highlighted[1] >= 253
                    && highlighted[2] <= 2
                    && highlighted[3] == 255,
                "canonical first Highlight did not multiply over the white page: {highlighted:?}"
            );
            if let Some(expected) = &first_pixels {
                assert_eq!(
                    image.as_raw(),
                    expected,
                    "identical ordered Highlight scenes must render deterministically"
                );
            } else {
                first_pixels = Some(image.into_raw());
            }
        }
        let pixels = first_pixels.expect("at least one Metal repeat");
        samples.push(json!({
            "highlight_count": count,
            "points_per_highlight": POINTS_PER_HIGHLIGHT,
            "scene_path_count": scene.paths.len(),
            "multiply_path_count": count,
            "path_vertex_count": path_vertex_count,
            "triangle_count": path_vertex_count / 3,
            "cpu_prepare_ns": cpu_prepare_ns,
            "cpu_prepare_median_ns": median(cpu_prepare_ns.clone()),
            "metal_complete_and_readback_ns": metal_complete_and_readback_ns,
            "metal_complete_and_readback_median_ns": median(metal_complete_and_readback_ns.clone()),
            "rgba_sha256": sha256_hex(&pixels),
        }));
    }

    println!(
        "BP_MANY_HIGHLIGHT_METAL_PROBE={}",
        serde_json::to_string(&json!({
            "schema_version": "bp-many-highlight-metal-probe-v1",
            "evidence_class": "development_probe",
            "release_budget_eligible": false,
            "platform": "macos",
            "architecture": std::env::consts::ARCH,
            "debug_assertions": cfg!(debug_assertions),
            "renderer": "GPUI Metal headless offscreen",
            "surface": { "width_px": WIDTH, "height_px": HEIGHT },
            "repeat_count": REPEATS,
            "style_contract": {
                "colour": "#ffff00ff",
                "width_pt_at_1x": 12,
                "opacity": 1,
                "blend": "multiply",
            },
            "timing_boundaries": {
                "cpu_prepare_ns": "64-point path generation, round-stroke tessellation, scene insertion and Scene::finish",
                "metal_complete_and_readback_ns": "GPUI offscreen Metal encode, draw, multisample resolve, ordered composite, command-buffer completion and managed-texture RGBA readback",
            },
            "not_measured": [
                "window presentation or scanout",
                "native input-to-presentation latency",
                "process CPU utilisation",
                "GPU utilisation or per-encoder GPU timestamps",
                "process or GPU memory",
                "settled idle use",
                "open-close lifecycle recovery",
            ],
            "samples": samples,
        }))
        .expect("probe receipt must serialize")
    );
}
