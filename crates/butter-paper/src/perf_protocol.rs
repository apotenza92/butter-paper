use std::io::{self, Write};

use serde_json::{Map, Value, json};

pub type PerfFields = Map<String, Value>;

pub fn fields(entries: impl IntoIterator<Item = (&'static str, Value)>) -> PerfFields {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}

pub trait PerfEventSink {
    fn write_line(&mut self, line: String);
}

#[derive(Default)]
pub struct StdoutSink;

impl PerfEventSink for StdoutSink {
    fn write_line(&mut self, line: String) {
        println!("{line}");
        let _ = io::stdout().flush();
    }
}

#[derive(Default)]
pub struct RecordingSink {
    lines: Vec<String>,
}

impl RecordingSink {
    pub fn into_lines(self) -> Vec<String> {
        self.lines
    }
}

impl PerfEventSink for RecordingSink {
    fn write_line(&mut self, line: String) {
        self.lines.push(line);
    }
}

pub struct PerfProtocol<S> {
    scenario: String,
    pid: u32,
    sink: S,
    last_t_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PerfProtocolError {
    ReservedField(String),
    InvalidTime,
    NonMonotonicTime { previous_ms: f64, actual_ms: f64 },
}

const RESERVED_FIELDS: [&str; 6] = [
    "schema_version",
    "runtime",
    "scenario",
    "event",
    "t_ms",
    "pid",
];

impl<S: PerfEventSink> PerfProtocol<S> {
    pub fn new(scenario: impl Into<String>, pid: u32, sink: S) -> Self {
        Self {
            scenario: scenario.into(),
            pid,
            sink,
            last_t_ms: None,
        }
    }

    pub fn emit_at(
        &mut self,
        event: &str,
        t_ms: f64,
        mut details: PerfFields,
    ) -> Result<(), PerfProtocolError> {
        if let Some(name) = RESERVED_FIELDS
            .iter()
            .find(|name| details.contains_key(**name))
        {
            return Err(PerfProtocolError::ReservedField((*name).to_owned()));
        }
        if !t_ms.is_finite() || t_ms < 0. {
            return Err(PerfProtocolError::InvalidTime);
        }
        if let Some(previous_ms) = self.last_t_ms
            && t_ms < previous_ms
        {
            return Err(PerfProtocolError::NonMonotonicTime {
                previous_ms,
                actual_ms: t_ms,
            });
        }
        let mut record = Map::new();
        record.insert("schema_version".into(), json!(1));
        record.insert("runtime".into(), json!("gpui"));
        record.insert("scenario".into(), json!(self.scenario));
        record.insert("event".into(), json!(event));
        record.insert("t_ms".into(), json!(t_ms));
        record.insert("pid".into(), json!(self.pid));
        record.append(&mut details);
        self.sink.write_line(Value::Object(record).to_string());
        self.last_t_ms = Some(t_ms);
        Ok(())
    }

    pub fn into_sink(self) -> S {
        self.sink
    }
}
