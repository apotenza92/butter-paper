//! Deterministic evidence gates for the representative engineering-sheet journey.

use serde::{Deserialize, Serialize};
use serde_json::Value;

const WORKLOAD_JSON: &str =
    include_str!("../../performance/comparison-workload-v4.materialized.json");
const EXPECTED_SCHEMA_VERSION: &str = "bp-comparison-workload-v2";
const EXPECTED_MANIFEST_ID: &str = "bp-perf-v4-decision-1";
const REQUIRED_SHELL_WIDTH: f32 = 1_200.0;
const REQUIRED_SHELL_HEIGHT: f32 = 800.0;
const MINIMUM_SETTLE_MS: f64 = 250.0;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FitMode {
    FitPage,
    FitWidth,
}

impl FitMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FitPage => "fit-page",
            Self::FitWidth => "fit-width",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FitModesPlan {
    pub command_id: String,
    pub modes: Vec<FitMode>,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CacheRecoveryPlan {
    pub command_id: String,
    pub cycles: usize,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct FitModeObservation {
    pub mode: FitMode,
    pub shell_width: f32,
    pub shell_height: f32,
    pub client_width: f32,
    pub client_height: f32,
    pub expected_zoom_percent: f32,
    pub applied_zoom_percent: f32,
    pub preset_current: bool,
    pub current_generation_presented: bool,
    pub settled_for_ms: f64,
    pub visible_tile_count: usize,
    pub maximum_visible_tiles: usize,
    pub settled_density: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FitModeReceipt {
    pub command_id: String,
    pub observation: FitModeObservation,
    pub milestones: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct AppResourceObservation {
    pub document_count: usize,
    pub tile_cache_bytes: usize,
    pub decoded_page_bytes: usize,
    pub renderer_resource_submission_bytes: usize,
}

impl AppResourceObservation {
    pub fn retained_render_bytes(self) -> usize {
        self.tile_cache_bytes
            .saturating_add(self.decoded_page_bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CacheRecoveryObservation {
    pub cycles_completed: usize,
    pub cache_limit_bytes: usize,
    pub decoded_limit_bytes: usize,
    pub before: AppResourceObservation,
    pub after: AppResourceObservation,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CacheRecoveryReceipt {
    pub command_id: String,
    pub observation: CacheRecoveryObservation,
    pub released_render_bytes: usize,
    pub milestones: Vec<String>,
}

pub fn embedded_fit_modes_plan() -> Result<FitModesPlan, String> {
    let command = engineering_command("engineering:fit-modes")?;
    let modes = command
        .get("modes")
        .and_then(Value::as_array)
        .ok_or_else(|| "engineering:fit-modes has no modes".to_string())?
        .iter()
        .map(|mode| serde_json::from_value(mode.clone()).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if modes != [FitMode::FitPage, FitMode::FitWidth] {
        return Err("engineering:fit-modes must retain Fit Page then Fit Width".into());
    }
    Ok(FitModesPlan {
        command_id: "engineering:fit-modes".into(),
        modes,
        expected_milestones: expected_milestones(&command)?,
    })
}

pub fn embedded_cache_recovery_plan() -> Result<CacheRecoveryPlan, String> {
    let command = engineering_command("engineering:cache-recovery")?;
    let cycles = command
        .get("cycles")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "engineering:cache-recovery has no valid cycle count".to_string())?;
    if cycles != 5 {
        return Err("engineering:cache-recovery must retain five cycles".into());
    }
    Ok(CacheRecoveryPlan {
        command_id: "engineering:cache-recovery".into(),
        cycles,
        expected_milestones: expected_milestones(&command)?,
    })
}

pub fn assess_fit_mode(
    plan: &FitModesPlan,
    observation: FitModeObservation,
) -> Result<FitModeReceipt, String> {
    if !plan.modes.contains(&observation.mode) {
        return Err("observed Fit mode is not in the frozen command".into());
    }
    if (observation.shell_width - REQUIRED_SHELL_WIDTH).abs() > 0.1
        || (observation.shell_height - REQUIRED_SHELL_HEIGHT).abs() > 0.1
    {
        return Err("Fit mode evidence did not use the 1200x800 shell".into());
    }
    if !observation.client_width.is_finite()
        || !observation.client_height.is_finite()
        || observation.client_width <= 0.0
        || observation.client_height <= 0.0
        || observation.client_width > observation.shell_width
        || observation.client_height > observation.shell_height
    {
        return Err("Fit mode client viewport is invalid for the requested shell".into());
    }
    if !observation.expected_zoom_percent.is_finite()
        || (observation.applied_zoom_percent - observation.expected_zoom_percent).abs() > 0.1
        || !observation.preset_current
    {
        return Err("Fit mode state is not current".into());
    }
    if !observation.current_generation_presented || observation.settled_for_ms < MINIMUM_SETTLE_MS {
        return Err("Fit mode output is not current and settled".into());
    }
    if observation.visible_tile_count > observation.maximum_visible_tiles {
        return Err("Fit mode visible tile set is not bounded".into());
    }
    if !observation.settled_density.is_finite() || observation.settled_density < 1.0 {
        return Err("Fit mode settled density is below one device pixel".into());
    }
    Ok(FitModeReceipt {
        command_id: plan.command_id.clone(),
        observation,
        milestones: plan.expected_milestones.clone(),
    })
}

pub fn assess_cache_recovery(
    plan: &CacheRecoveryPlan,
    observation: CacheRecoveryObservation,
) -> Result<CacheRecoveryReceipt, String> {
    if observation.cycles_completed != plan.cycles {
        return Err(format!(
            "engineering cache completed {} of {} cycles",
            observation.cycles_completed, plan.cycles,
        ));
    }
    if observation.before.document_count != 1 {
        return Err("engineering cache recovery requires one open document before close".into());
    }
    if observation.before.tile_cache_bytes > observation.cache_limit_bytes {
        return Err("engineering tile cache exceeded its declared byte limit".into());
    }
    if observation.before.decoded_page_bytes > observation.decoded_limit_bytes {
        return Err("engineering decoded page cache exceeded its declared byte limit".into());
    }
    if observation.before.renderer_resource_submission_bytes == 0 {
        return Err("engineering cache cycles recorded no renderer resource submissions".into());
    }
    if observation.after.document_count != 0
        || observation.after.tile_cache_bytes != 0
        || observation.after.decoded_page_bytes != 0
        || observation.after.renderer_resource_submission_bytes != 0
    {
        return Err("engineering document resources remained after close".into());
    }
    let released_render_bytes = observation.before.retained_render_bytes();
    if released_render_bytes == 0 {
        return Err("engineering cache recovery released no retained render bytes".into());
    }
    Ok(CacheRecoveryReceipt {
        command_id: plan.command_id.clone(),
        observation,
        released_render_bytes,
        milestones: plan.expected_milestones.clone(),
    })
}

fn workload() -> Result<Value, String> {
    let workload: Value = serde_json::from_str(WORKLOAD_JSON).map_err(|error| error.to_string())?;
    if workload.get("schema_version").and_then(Value::as_str) != Some(EXPECTED_SCHEMA_VERSION)
        || workload.get("manifest_id").and_then(Value::as_str) != Some(EXPECTED_MANIFEST_ID)
    {
        return Err("embedded v4 workload identity is invalid".into());
    }
    Ok(workload)
}

fn engineering_command(command_id: &str) -> Result<Value, String> {
    let workload = workload()?;
    workload
        .get("journeys")
        .and_then(Value::as_array)
        .and_then(|journeys| {
            journeys.iter().find(|journey| {
                journey.get("id").and_then(Value::as_str) == Some("engineering-sheet-v1")
            })
        })
        .and_then(|journey| journey.get("commands"))
        .and_then(Value::as_array)
        .and_then(|commands| {
            commands
                .iter()
                .find(|command| command.get("id").and_then(Value::as_str) == Some(command_id))
        })
        .cloned()
        .ok_or_else(|| format!("embedded v4 workload has no {command_id}"))
}

fn expected_milestones(command: &Value) -> Result<Vec<String>, String> {
    command
        .get("expected_milestones")
        .and_then(Value::as_array)
        .ok_or_else(|| "engineering command has no expected milestones".to_string())?
        .iter()
        .map(|milestone| {
            milestone
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "engineering milestone is not a string".to_string())
        })
        .collect()
}
