use std::{
    io::{self, Write},
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value, json};

static ORIGIN: OnceLock<Instant> = OnceLock::new();
static SCENARIO: OnceLock<Option<String>> = OnceLock::new();

pub fn init() {
    let _ = ORIGIN.set(Instant::now());
    let _ = SCENARIO.set(std::env::var("BP_GPUI_PERF_SCENARIO").ok());
    emit("process-start", Map::new());
}

pub fn enabled() -> bool {
    scenario().is_some()
}

pub fn scenario() -> Option<&'static str> {
    SCENARIO
        .get_or_init(|| std::env::var("BP_GPUI_PERF_SCENARIO").ok())
        .as_deref()
}

pub fn elapsed_ms() -> f64 {
    ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

pub fn fields(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

pub fn emit(event: &str, mut details: Map<String, Value>) {
    if !enabled() {
        return;
    }
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .unwrap_or_default();
    let mut record = Map::new();
    record.insert("schema_version".into(), json!(1));
    record.insert("runtime".into(), json!("gpui"));
    record.insert("scenario".into(), json!(scenario()));
    record.insert("event".into(), json!(event));
    record.insert("t_ms".into(), json!(elapsed_ms()));
    record.insert("epoch_ms".into(), json!(epoch_ms));
    record.insert("pid".into(), json!(std::process::id()));
    record.append(&mut details);
    println!("{}", Value::Object(record));
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_builder_keeps_typed_values() {
        let value = fields([("page", json!(8)), ("cache", json!("hit"))]);
        assert_eq!(value.get("page"), Some(&json!(8)));
        assert_eq!(value.get("cache"), Some(&json!("hit")));
    }
}
