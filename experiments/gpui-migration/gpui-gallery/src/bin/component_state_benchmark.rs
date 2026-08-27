use std::time::Instant;

#[path = "../component_model.rs"]
mod component_model;

use component_model::ShellModel;
use serde_json::json;

const BENCHMARK_ITERATIONS: usize = 250_000;

fn run_state_benchmark() {
    let started_at = Instant::now();
    let mut model = ShellModel::default();
    for index in 0..BENCHMARK_ITERATIONS {
        model.change_zoom(if index % 2 == 0 { 1 } else { -1 });
        model.select_fit_mode(index % 2);
        model.select_scroll_mode(usize::from(index % 3 == 0));
        model.active_document = index % 2;
        model.active_tool = index % 3;
    }
    let duration = started_at.elapsed();
    println!(
        "{}",
        json!({
            "schema_version": 1,
            "runtime": "butter-paper-direct-gpui",
            "scenario": "component-state-benchmark",
            "iterations": BENCHMARK_ITERATIONS,
            "duration_ms": duration.as_secs_f64() * 1000.0,
            "operations_per_second": BENCHMARK_ITERATIONS as f64 / duration.as_secs_f64(),
            "final_state": {
                "active_document": model.active_document,
                "active_tool": model.active_tool,
                "continuous": model.continuous,
                "fit_mode": format!("{:?}", model.fit_mode),
                "zoom_percent": model.zoom_percent,
            },
        })
    );
}

fn main() {
    run_state_benchmark();
}
