use butter_paper_gpui_gallery::viewer::{
    CachePolicy, PageGeometry, PixelRect, Rect, RenderInput, RenderPlanner, RenderSource,
    TileCache, TileRequest, ViewportGeometry,
};

fn source(document_id: u64) -> RenderSource {
    RenderSource {
        document_id,
        revision: 1,
    }
}

#[test]
fn large_sheet_at_1600_percent_plans_only_bounded_visible_tiles() {
    let pages = [PageGeometry::new(1, 1_440.0, 1_080.0)];
    let mut planner = RenderPlanner::default();
    let plan = planner.plan(RenderInput {
        source: source(1),
        pages: &pages,
        zoom_percent: 1_600.0,
        device_scale: 2.0,
        page_gap: 24.0,
        viewport: ViewportGeometry {
            width: 1_280.0,
            height: 800.0,
            scroll_x: 12_000.0,
            scroll_y: 7_000.0,
            visible_rect: Rect::new(0.0, 0.0, 1_280.0, 800.0),
        },
    });

    assert!(!plan.tiles.is_empty());
    assert!(plan.tiles.len() <= 12);
    assert!(plan.tiles.iter().all(|tile| tile.crop.width <= 1_024));
    assert!(plan.tiles.iter().all(|tile| tile.crop.height <= 1_024));
    assert!(plan.requested_bytes <= 12 * 1_024 * 1_024 * 4);
    assert_eq!(plan.page_layouts[0].device_width, 46_080);
    assert_eq!(plan.page_layouts[0].device_height, 34_560);
    assert_eq!(plan.cache.max_bytes, 256 * 1_024 * 1_024);
}

#[test]
fn mixed_page_layout_uses_each_pages_geometry() {
    let pages = [
        PageGeometry::new(1, 612.0, 792.0),
        PageGeometry::new(2, 1_440.0, 1_080.0),
        PageGeometry::new(3, 792.0, 612.0),
    ];
    let mut planner = RenderPlanner::default();
    let second_page_y = 24.0 + 792.0 * 16.0 + 24.0;
    let plan = planner.plan(RenderInput {
        source: source(1),
        pages: &pages,
        zoom_percent: 1_600.0,
        device_scale: 2.0,
        page_gap: 24.0,
        viewport: ViewportGeometry {
            width: 1_280.0,
            height: 800.0,
            scroll_x: 1_048.0,
            scroll_y: second_page_y + 1_024.0,
            visible_rect: Rect::new(0.0, 0.0, 800.0, 600.0),
        },
    });

    assert_eq!(plan.visible_pages, vec![2]);
    assert_eq!(plan.page_layouts[0].device_width, 19_584);
    assert_eq!(plan.page_layouts[1].device_width, 46_080);
    assert_eq!(plan.page_layouts[2].device_width, 25_344);
    assert!(plan.tiles.iter().all(|tile| tile.page == 2));
    assert!(plan.tiles.iter().all(|tile| tile.crop.x < 46_080));
}

#[test]
fn scrolling_mixed_pages_reports_exact_offsets_and_current_page() {
    let pages = [
        PageGeometry::new(1, 612.0, 792.0),
        PageGeometry::new(2, 1_440.0, 1_080.0),
        PageGeometry::new(3, 792.0, 612.0),
    ];
    let mut planner = RenderPlanner::default();
    let plan = planner.plan(RenderInput {
        source: source(1),
        pages: &pages,
        zoom_percent: 100.0,
        device_scale: 1.0,
        page_gap: 24.0,
        viewport: ViewportGeometry {
            width: 1_280.0,
            height: 200.0,
            scroll_x: 0.0,
            scroll_y: 780.0,
            visible_rect: Rect::new(0.0, 0.0, 1_280.0, 200.0),
        },
    });

    assert_eq!(plan.page_layouts[0].logical_rect.y, 24.0);
    assert_eq!(plan.page_layouts[1].logical_rect.y, 840.0);
    assert_eq!(plan.page_layouts[2].logical_rect.y, 1_944.0);
    assert_eq!(plan.page_layouts[0].logical_rect.x, 438.0);
    assert_eq!(plan.page_layouts[1].logical_rect.x, 24.0);
    assert_eq!(plan.page_layouts[2].logical_rect.x, 348.0);
    assert_eq!(plan.visible_pages, vec![1, 2]);
    assert_eq!(plan.current_page, Some(2));
    assert_eq!(plan.total_height, 2_580.0);
}

#[test]
fn equivalent_visible_tile_plans_keep_the_current_render_generation() {
    let pages = [PageGeometry::new(1, 612.0, 792.0)];
    let input = || RenderInput {
        source: source(1),
        pages: &pages,
        zoom_percent: 400.0,
        device_scale: 2.0,
        page_gap: 24.0,
        viewport: ViewportGeometry {
            width: 1_280.0,
            height: 800.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            visible_rect: Rect::new(0.0, 0.0, 1_280.0, 800.0),
        },
    };
    let mut planner = RenderPlanner::default();
    let old = planner.plan(input());
    let current = planner.plan(input());

    assert_eq!(old.generation, current.generation);
    assert!(planner.accepts(old.generation));
    assert!(planner.accepts(current.generation));
    planner.cancel();
    assert!(!planner.accepts(current.generation));
}

#[test]
fn changing_the_visible_tile_set_advances_the_render_generation() {
    let pages = [PageGeometry::new(1, 1_440.0, 1_080.0)];
    let input = |scroll_x| RenderInput {
        source: source(1),
        pages: &pages,
        zoom_percent: 1_600.0,
        device_scale: 1.0,
        page_gap: 24.0,
        viewport: ViewportGeometry {
            width: 800.0,
            height: 600.0,
            scroll_x,
            scroll_y: 0.0,
            visible_rect: Rect::new(0.0, 0.0, 800.0, 600.0),
        },
    };
    let mut planner = RenderPlanner::default();
    let first = planner.plan(input(0.0));
    let shifted_within_the_same_tiles = planner.plan(input(100.0));
    let shifted_to_new_tiles = planner.plan(input(1_100.0));

    assert_eq!(first.generation, shifted_within_the_same_tiles.generation);
    assert_ne!(first.generation, shifted_to_new_tiles.generation);
    assert!(!planner.accepts(first.generation));
    assert!(planner.accepts(shifted_to_new_tiles.generation));
}

#[test]
fn tile_cache_evicts_least_recently_used_entries_before_byte_limit() {
    let policy = CachePolicy {
        tile_edge: 2,
        max_tiles_per_plan: 4,
        max_bytes: 32,
    };
    let tile = |x| TileRequest {
        source: source(1),
        generation: 1,
        page: 1,
        zoom_tenths: 1_000,
        device_scale_millis: 1_000,
        rotation_quarter_turns: 0,
        crop: PixelRect {
            x,
            y: 0,
            width: 2,
            height: 2,
        },
    };
    let mut cache = TileCache::new(policy);
    assert!(cache.insert(tile(0), "left", 16));
    assert!(cache.insert(tile(2), "middle", 16));
    assert_eq!(cache.get(tile(0)), Some(&"left"));
    assert!(cache.insert(tile(4), "right", 16));

    assert!(cache.contains(tile(0)));
    assert!(!cache.contains(tile(2)));
    assert!(cache.contains(tile(4)));
    assert_eq!(cache.bytes(), 32);
}

#[test]
fn tile_cache_never_reuses_across_documents_revisions_or_render_scales() {
    let policy = CachePolicy {
        tile_edge: 2,
        max_tiles_per_plan: 8,
        max_bytes: 128,
    };
    let tile = |document_id, revision, zoom_tenths, device_scale_millis| TileRequest {
        source: RenderSource {
            document_id,
            revision,
        },
        generation: 1,
        page: 1,
        zoom_tenths,
        device_scale_millis,
        rotation_quarter_turns: 0,
        crop: PixelRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
    };
    let mut cache = TileCache::new(policy);
    let original = tile(1, 4, 1_000, 1_000);
    assert!(cache.insert(original, "original", 16));

    assert_eq!(cache.peek(original), Some(&"original"));
    assert_eq!(cache.peek(tile(2, 4, 1_000, 1_000)), None);
    assert_eq!(cache.peek(tile(1, 5, 1_000, 1_000)), None);
    assert_eq!(cache.peek(tile(1, 4, 2_000, 1_000)), None);
    assert_eq!(cache.peek(tile(1, 4, 1_000, 2_000)), None);
}

#[test]
fn closing_a_document_can_release_every_byte_accounted_tile() {
    let mut cache = TileCache::new(CachePolicy {
        tile_edge: 2,
        max_tiles_per_plan: 4,
        max_bytes: 64,
    });
    let tile = TileRequest {
        source: source(99),
        generation: 1,
        page: 1,
        zoom_tenths: 16_000,
        device_scale_millis: 1_000,
        rotation_quarter_turns: 0,
        crop: PixelRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
    };
    assert!(cache.insert(tile, "tile", 16));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.bytes(), 16);

    cache.clear();

    assert!(cache.is_empty());
    assert_eq!(cache.bytes(), 0);
    assert_eq!(cache.peek(tile), None);
}
