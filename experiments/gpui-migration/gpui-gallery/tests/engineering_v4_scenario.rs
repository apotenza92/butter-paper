use butter_paper_gpui_gallery::engineering_v4_scenario::{
    AppResourceObservation, CacheRecoveryObservation, FitMode, FitModeObservation,
    assess_cache_recovery, assess_fit_mode, embedded_cache_recovery_plan, embedded_fit_modes_plan,
};

#[test]
fn fit_modes_require_the_real_1200_by_800_shell_and_current_settled_output() {
    let plan = embedded_fit_modes_plan().expect("the v4 Fit modes plan should parse");
    assert_eq!(plan.command_id, "engineering:fit-modes");
    assert_eq!(plan.modes, [FitMode::FitPage, FitMode::FitWidth]);

    let fit_page = assess_fit_mode(
        &plan,
        FitModeObservation {
            mode: FitMode::FitPage,
            shell_width: 1_200.0,
            shell_height: 800.0,
            client_width: 1_198.0,
            client_height: 777.0,
            expected_zoom_percent: 50.0,
            applied_zoom_percent: 50.0,
            preset_current: true,
            current_generation_presented: true,
            settled_for_ms: 250.0,
            visible_tile_count: 2,
            maximum_visible_tiles: 32,
            settled_density: 1.0,
        },
    )
    .expect("a current Fit Page observation should qualify");
    assert_eq!(
        fit_page.milestones,
        [
            "fit-state-current",
            "visible-tiles-bounded",
            "settled-density-at-least-1",
        ]
    );

    let mut stale = fit_page.observation;
    stale.current_generation_presented = false;
    assert!(assess_fit_mode(&plan, stale).is_err());
}

#[test]
fn engineering_cache_recovery_requires_five_cycles_and_released_app_resources() {
    let plan = embedded_cache_recovery_plan().expect("the v4 cache recovery plan should parse");
    assert_eq!(plan.command_id, "engineering:cache-recovery");
    assert_eq!(plan.cycles, 5);

    let receipt = assess_cache_recovery(
        &plan,
        CacheRecoveryObservation {
            cycles_completed: 5,
            cache_limit_bytes: 256 * 1_024 * 1_024,
            decoded_limit_bytes: 128 * 1_024 * 1_024,
            before: AppResourceObservation {
                document_count: 1,
                tile_cache_bytes: 32 * 1_024 * 1_024,
                decoded_page_bytes: 8 * 1_024 * 1_024,
                renderer_resource_submission_bytes: 40 * 1_024 * 1_024,
            },
            after: AppResourceObservation {
                document_count: 0,
                tile_cache_bytes: 0,
                decoded_page_bytes: 0,
                renderer_resource_submission_bytes: 0,
            },
        },
    )
    .expect("closed engineering resources should qualify");
    assert_eq!(receipt.released_render_bytes, 40 * 1_024 * 1_024);
    assert_eq!(
        receipt.milestones,
        [
            "declared-cache-byte-limit-held",
            "decoded-byte-limit-held",
            "renderer-resource-submission-bytes-exact",
            "memory-recovery-recorded",
        ]
    );

    let mut retained = receipt.observation;
    retained.after.tile_cache_bytes = 1;
    assert!(assess_cache_recovery(&plan, retained).is_err());
}
