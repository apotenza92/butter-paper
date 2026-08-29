use butter_paper_gpui_gallery::viewer::{
    CachePolicy, CadOrganisation, PageGeometry, PixelRect, Rect, RenderInput, RenderLayout,
    RenderPlanner, RenderSource, TileCache, TileRequest, ViewportGeometry,
};

#[test]
fn cad_layout_columns_and_rows_match_the_frozen_electron_grouping_contract() {
    let pages = (0..5)
        .map(|page| PageGeometry::new(page, 100., 200.))
        .collect::<Vec<_>>();
    let input = || RenderInput {
        source: RenderSource {
            document_id: 1,
            revision: 1,
        },
        pages: &pages,
        zoom_percent: 100.,
        device_scale: 1.,
        page_gap: 24.,
        viewport: butter_paper_gpui_gallery::viewer::ViewportGeometry {
            width: 800.,
            height: 600.,
            scroll_x: 0.,
            scroll_y: 0.,
            visible_rect: Rect::new(0., 0., 800., 600.),
        },
    };

    let mut planner = RenderPlanner::default();
    let columns = planner.plan_with_layout(
        input(),
        RenderLayout::Cad {
            organisation: CadOrganisation::Columns,
            pages_per_lane: 2,
        },
    );
    assert_eq!(
        columns
            .page_layouts
            .iter()
            .map(|page| (page.column_index, page.row_index))
            .collect::<Vec<_>>(),
        [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)]
    );
    assert!(columns.total_width >= 396.);

    let rows = planner.plan_with_layout(
        input(),
        RenderLayout::Cad {
            organisation: CadOrganisation::Rows,
            pages_per_lane: 3,
        },
    );
    assert_eq!(
        rows.page_layouts
            .iter()
            .map(|page| (page.column_index, page.row_index))
            .collect::<Vec<_>>(),
        [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1)]
    );
    assert!(rows.total_height >= 448.);
}

fn cad_plan(
    pages: &[PageGeometry],
    organisation: CadOrganisation,
    pages_per_lane: usize,
    viewport: ViewportGeometry,
) -> butter_paper_gpui_gallery::viewer::RenderPlan {
    RenderPlanner::default().plan_with_layout(
        RenderInput {
            source: source(1),
            pages,
            zoom_percent: 100.,
            device_scale: 1.,
            page_gap: 10.,
            viewport,
        },
        RenderLayout::Cad {
            organisation,
            pages_per_lane,
        },
    )
}

#[test]
fn cad_layout_current_page_follows_horizontal_rows_and_vertical_columns() {
    let pages = (0..4)
        .map(|page| PageGeometry::new(page, 100., 100.))
        .collect::<Vec<_>>();

    let horizontal = cad_plan(
        &pages,
        CadOrganisation::Rows,
        2,
        ViewportGeometry {
            width: 100.,
            height: 100.,
            scroll_x: 100.,
            scroll_y: 0.,
            visible_rect: Rect::new(0., 0., 100., 100.),
        },
    );
    let vertical = cad_plan(
        &pages,
        CadOrganisation::Columns,
        2,
        ViewportGeometry {
            width: 220.,
            height: 100.,
            scroll_x: 0.,
            scroll_y: 100.,
            visible_rect: Rect::new(0., 0., 220., 100.),
        },
    );

    assert_eq!(horizontal.current_page, Some(1));
    assert_eq!(vertical.current_page, Some(1));
}

#[test]
fn cad_layout_current_page_uses_intersection_then_source_order_for_focus_ties() {
    let unequal_pages = [
        PageGeometry::new(10, 100., 20.),
        PageGeometry::new(20, 100., 100.),
    ];
    let larger_intersection = cad_plan(
        &unequal_pages,
        CadOrganisation::Rows,
        2,
        ViewportGeometry {
            width: 20.,
            height: 60.,
            scroll_x: 105.,
            scroll_y: 0.,
            visible_rect: Rect::new(0., 0., 20., 60.),
        },
    );
    let equal_pages = [
        PageGeometry::new(10, 100., 100.),
        PageGeometry::new(20, 100., 100.),
    ];
    let source_order = cad_plan(
        &equal_pages,
        CadOrganisation::Rows,
        2,
        ViewportGeometry {
            width: 20.,
            height: 100.,
            scroll_x: 105.,
            scroll_y: 0.,
            visible_rect: Rect::new(0., 0., 20., 100.),
        },
    );

    assert_eq!(larger_intersection.current_page, Some(20));
    assert_eq!(source_order.current_page, Some(10));
}

#[test]
fn cad_layout_current_page_prefers_a_partially_visible_page_containing_the_center() {
    let pages = [
        PageGeometry::new(10, 100., 100.),
        PageGeometry::new(20, 100., 100.),
    ];
    let plan = cad_plan(
        &pages,
        CadOrganisation::Rows,
        2,
        ViewportGeometry {
            width: 100.,
            height: 100.,
            scroll_x: 80.,
            scroll_y: 0.,
            visible_rect: Rect::new(0., 0., 100., 100.),
        },
    );

    assert_eq!(plan.current_page, Some(20));
}

#[test]
fn cad_layout_empty_document_has_no_current_or_visible_page() {
    let plan = cad_plan(
        &[],
        CadOrganisation::Rows,
        2,
        ViewportGeometry {
            width: 100.,
            height: 100.,
            scroll_x: 0.,
            scroll_y: 0.,
            visible_rect: Rect::new(0., 0., 100., 100.),
        },
    );

    assert_eq!(plan.current_page, None);
    assert!(plan.visible_pages.is_empty());
}

#[test]
fn cad_layout_tile_cap_keeps_all_visible_pages_and_prioritizes_focus() {
    let pages = (0..40)
        .map(|page| PageGeometry::new(page, 10., 10.))
        .collect::<Vec<_>>();
    let plan = cad_plan(
        &pages,
        CadOrganisation::Rows,
        40,
        ViewportGeometry {
            width: 810.,
            height: 30.,
            scroll_x: 0.,
            scroll_y: 0.,
            visible_rect: Rect::new(0., 0., 810., 30.),
        },
    );

    assert_eq!(plan.visible_pages, (0..40).collect::<Vec<_>>());
    assert_eq!(plan.tiles.len(), plan.cache.max_tiles_per_plan);
    assert_eq!(plan.tiles[0].page, plan.current_page.unwrap());
    assert_ne!(plan.tiles[0].page, 0);
}

#[test]
fn cad_layout_tile_cap_round_robins_across_focus_ranked_visible_pages() {
    let pages = [
        PageGeometry::new(10, 40_960., 4_096.),
        PageGeometry::new(20, 40_960., 4_096.),
    ];
    let plan = cad_plan(
        &pages,
        CadOrganisation::Rows,
        2,
        ViewportGeometry {
            width: 81_950.,
            height: 4_116.,
            scroll_x: 0.,
            scroll_y: 0.,
            visible_rect: Rect::new(0., 0., 81_950., 4_116.),
        },
    );

    assert_eq!(plan.tiles.len(), plan.cache.max_tiles_per_plan);
    assert_eq!(plan.tiles[0].page, plan.current_page.unwrap());
    assert_ne!(plan.tiles[0].page, plan.tiles[1].page);
    assert_eq!(plan.tiles[0].page, plan.tiles[2].page);
    assert_eq!(plan.tiles[1].page, plan.tiles[3].page);
}

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
