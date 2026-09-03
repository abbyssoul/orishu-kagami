use serde_json::Value;
use tabled::{builder::Builder, settings::Style};

use crate::OutputMode;

pub struct Formatter {
    mode: OutputMode,
}

impl Formatter {
    pub fn new(mode: OutputMode) -> Self {
        Self { mode }
    }

    /// Print a plain status message (used for acknowledgement-only commands).
    pub fn message(&self, msg: &str) {
        println!("{msg}");
    }

    /// Print a single structured record (key-value object).
    pub fn record(&self, value: Value) {
        match &self.mode {
            OutputMode::Table => print_kv_table(&value),
            OutputMode::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).expect("JSON serialization failed")
                );
            }
            OutputMode::Yaml => {
                print!(
                    "{}",
                    serde_yaml::to_string(&value).expect("YAML serialization failed")
                );
            }
        }
    }

    /// Print a list of structured records as a table / JSON array / YAML sequence.
    pub fn list(&self, values: Vec<Value>) {
        match &self.mode {
            OutputMode::Table => print_list_table(values),
            OutputMode::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&values).expect("JSON serialization failed")
                );
            }
            OutputMode::Yaml => {
                print!(
                    "{}",
                    serde_yaml::to_string(&values).expect("YAML serialization failed")
                );
            }
        }
    }
}

// ── Table helpers ─────────────────────────────────────────────────────────────

fn display_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Two-column key-value table for single-record output.
fn print_kv_table(value: &Value) {
    if let Some(map) = value.as_object() {
        let mut builder = Builder::default();
        for (k, v) in map {
            builder.push_record([k.as_str(), &display_value(v)]);
        }
        println!("{}", builder.build().with(Style::blank()));
    } else {
        println!("{}", display_value(value));
    }
}

/// Multi-column table for list output.  The column headers are taken from the
/// keys of the first item, so all items must share the same key order.
fn print_list_table(values: Vec<Value>) {
    if values.is_empty() {
        println!("(empty)");
        return;
    }

    let mut builder = Builder::default();

    if let Some(first) = values.first().and_then(|v| v.as_object()) {
        let headers: Vec<String> = first.keys().cloned().collect();
        builder.push_record(headers);
    }

    for value in &values {
        if let Some(map) = value.as_object() {
            let row: Vec<String> = map.values().map(display_value).collect();
            builder.push_record(row);
        }
    }

    println!("{}", builder.build().with(Style::blank()));
}
