//! Value tracing and origin tracking for debugging configuration issues.
//!
//! This module provides types and methods for tracking where configuration values
//! came from and their override history across multiple sources.
//!
//! # Overview
//!
//! When debugging configuration issues, especially in production, it's crucial
//! to know where each value came from. With multiple sources (defaults, file,
//! environment), a value might be set in one place and overridden in another.
//! Value tracing shows the complete history of each configuration path.
//!
//! # Usage
//!
//! ```ignore
//! use premortem::{Config, TracedConfig};
//!
//! let traced = Config::<AppConfig>::builder()
//!     .source(Defaults::from(AppConfig::default()))
//!     .source(Toml::file("config.toml"))
//!     .source(Env::prefix("APP_"))
//!     .build_traced()?;
//!
//! // Query specific path
//! if let Some(trace) = traced.trace("database.host") {
//!     println!("database.host = {:?}", trace.final_value.value);
//!     println!("  from: {}", trace.final_value.source);
//! }
//!
//! // Check for overrides
//! if traced.was_overridden("database.host") {
//!     println!("Warning: database.host was overridden");
//! }
//!
//! // Generate full report
//! println!("{}", traced.trace_report());
//! ```

use std::collections::BTreeMap;
use std::fmt;

use crate::config::Config;
use crate::error::SourceLocation;
use crate::value::Value;

/// A value with its source information.
#[derive(Debug, Clone)]
pub struct TracedValue {
    /// The value at this source
    pub value: Value,
    /// Where this value came from
    pub source: SourceLocation,
    /// Whether this value was used (not overridden)
    pub is_final: bool,
}

impl TracedValue {
    /// Create a new traced value.
    pub fn new(value: Value, source: SourceLocation, is_final: bool) -> Self {
        Self {
            value,
            source,
            is_final,
        }
    }
}

/// Trace of a single configuration value.
#[derive(Debug, Clone)]
pub struct ValueTrace {
    /// The final value (from highest priority source)
    pub final_value: TracedValue,
    /// All values from all sources, in priority order (lowest first)
    pub history: Vec<TracedValue>,
}

impl ValueTrace {
    /// Create a new value trace from a history of traced values.
    ///
    /// The last value in the history is considered the final value.
    /// The `is_final` flag is automatically set on the last value.
    pub fn new(mut history: Vec<TracedValue>) -> Option<Self> {
        if history.is_empty() {
            return None;
        }

        // Mark the last value as final
        if let Some(last) = history.last_mut() {
            last.is_final = true;
        }

        let final_value = history.last().cloned()?;

        Some(Self {
            final_value,
            history,
        })
    }

    /// Check if this value was overridden (has more than one source).
    pub fn was_overridden(&self) -> bool {
        self.history.len() > 1
    }

    /// Get the number of sources that provided this value.
    pub fn source_count(&self) -> usize {
        self.history.len()
    }
}

impl fmt::Display for ValueTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Final: {:?} (from {})",
            self.final_value.value, self.final_value.source
        )?;

        if self.history.len() > 1 {
            writeln!(f, "History:")?;
            for val in &self.history {
                let marker = if val.is_final { "→" } else { " " };
                writeln!(f, "  {} [{}] {:?}", marker, val.source, val.value)?;
            }
        }

        Ok(())
    }
}

/// Configuration with tracing information.
///
/// This type wraps a `Config<T>` with additional trace data that shows
/// where each configuration value came from and its override history.
#[derive(Debug)]
pub struct TracedConfig<T> {
    config: Config<T>,
    traces: BTreeMap<String, ValueTrace>,
}

impl<T> TracedConfig<T> {
    /// Create a new traced config from a config and traces.
    pub fn new(config: Config<T>, traces: BTreeMap<String, ValueTrace>) -> Self {
        Self { config, traces }
    }

    /// Get reference to the configuration.
    pub fn value(&self) -> &T {
        self.config.get()
    }

    /// Get reference to the inner Config.
    pub fn config(&self) -> &Config<T> {
        &self.config
    }

    /// Consume and return the configuration value.
    pub fn into_inner(self) -> T {
        self.config.into_inner()
    }

    /// Consume and return the inner Config.
    pub fn into_config(self) -> Config<T> {
        self.config
    }

    /// Get the trace for a specific path.
    pub fn trace(&self, path: &str) -> Option<&ValueTrace> {
        self.traces.get(path)
    }

    /// Check if a path was overridden by a higher-priority source.
    pub fn was_overridden(&self, path: &str) -> bool {
        self.traces
            .get(path)
            .map(|t| t.history.len() > 1)
            .unwrap_or(false)
    }

    /// Get all traces.
    pub fn traces(&self) -> impl Iterator<Item = (&str, &ValueTrace)> {
        self.traces.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Get paths that were overridden.
    pub fn overridden_paths(&self) -> impl Iterator<Item = &str> {
        self.traces
            .iter()
            .filter(|(_, t)| t.history.len() > 1)
            .map(|(k, _)| k.as_str())
    }

    /// Get all traced paths.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.traces.keys().map(|k| k.as_str())
    }

    /// Get the number of traced paths.
    pub fn trace_count(&self) -> usize {
        self.traces.len()
    }

    /// Generate a human-readable trace report.
    pub fn trace_report(&self) -> String {
        let mut report = String::new();

        for (path, trace) in &self.traces {
            report.push_str(&format!("{} = {:?}\n", path, trace.final_value.value));

            for val in &trace.history {
                let marker = if val.is_final { "✓" } else { "○" };
                let override_note = if !val.is_final { " <- overridden" } else { "" };
                report.push_str(&format!(
                    "  {} [{}] {:?}{}\n",
                    marker, val.source, val.value, override_note
                ));
            }
            report.push('\n');
        }

        report
    }
}

impl<T> std::ops::Deref for TracedConfig<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.config.get()
    }
}

impl<T> AsRef<T> for TracedConfig<T> {
    fn as_ref(&self) -> &T {
        self.config.get()
    }
}

/// Builder for collecting trace data during config building.
///
/// This is used internally by `ConfigBuilder::build_traced()`.
#[derive(Debug, Default)]
pub struct TraceBuilder {
    /// Values collected from all sources, keyed by path.
    /// Each path maps to a list of (value, source) pairs in source order.
    values: BTreeMap<String, Vec<TracedValue>>,
}

impl TraceBuilder {
    /// Create a new trace builder.
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Add a value from a source.
    pub fn add_value(&mut self, path: String, value: Value, source: SourceLocation) {
        self.values
            .entry(path)
            .or_default()
            .push(TracedValue::new(value, source, false));
    }

    /// Build the final traces map.
    pub fn build(self) -> BTreeMap<String, ValueTrace> {
        self.values
            .into_iter()
            .filter_map(|(path, history)| ValueTrace::new(history).map(|trace| (path, trace)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_traced_value_new() {
        let tv = TracedValue::new(
            Value::String("localhost".to_string()),
            SourceLocation::new("config.toml"),
            false,
        );

        assert_eq!(tv.value.as_str(), Some("localhost"));
        assert_eq!(tv.source.source, "config.toml");
        assert!(!tv.is_final);
    }

    #[test]
    fn test_value_trace_new() {
        let history = vec![
            TracedValue::new(
                Value::String("default".to_string()),
                SourceLocation::new("defaults"),
                false,
            ),
            TracedValue::new(
                Value::String("override".to_string()),
                SourceLocation::new("config.toml"),
                false,
            ),
        ];

        let trace = ValueTrace::new(history).unwrap();

        assert_eq!(trace.final_value.value.as_str(), Some("override"));
        assert!(trace.final_value.is_final);
        assert!(trace.was_overridden());
        assert_eq!(trace.source_count(), 2);
    }

    #[test]
    fn test_value_trace_single_source() {
        let history = vec![TracedValue::new(
            Value::Integer(8080),
            SourceLocation::new("config.toml"),
            false,
        )];

        let trace = ValueTrace::new(history).unwrap();

        assert!(!trace.was_overridden());
        assert_eq!(trace.source_count(), 1);
    }

    #[test]
    fn test_value_trace_empty_history() {
        let trace = ValueTrace::new(vec![]);
        assert!(trace.is_none());
    }

    #[test]
    fn test_value_trace_display() {
        let history = vec![
            TracedValue::new(
                Value::String("localhost".to_string()),
                SourceLocation::new("defaults"),
                false,
            ),
            TracedValue::new(
                Value::String("prod-db".to_string()),
                SourceLocation::new("env:DB_HOST"),
                true,
            ),
        ];

        let trace = ValueTrace::new(history).unwrap();
        let display = format!("{}", trace);

        assert!(display.contains("Final:"));
        assert!(display.contains("prod-db"));
        assert!(display.contains("History:"));
    }

    #[test]
    fn test_traced_config_basic() {
        #[allow(dead_code)]
        #[derive(Debug)]
        struct TestConfig {
            host: String,
            port: i64,
        }

        let config = Config::new(TestConfig {
            host: "localhost".to_string(),
            port: 8080,
        });

        let mut traces = BTreeMap::new();
        traces.insert(
            "host".to_string(),
            ValueTrace::new(vec![TracedValue::new(
                Value::String("localhost".to_string()),
                SourceLocation::new("config.toml"),
                false,
            )])
            .unwrap(),
        );

        let traced = TracedConfig::new(config, traces);

        assert_eq!(traced.value().host, "localhost");
        assert_eq!(traced.trace_count(), 1);
        assert!(traced.trace("host").is_some());
        assert!(traced.trace("nonexistent").is_none());
    }

    #[test]
    fn test_traced_config_was_overridden() {
        #[allow(dead_code)]
        #[derive(Debug)]
        struct TestConfig {
            value: String,
        }

        let config = Config::new(TestConfig {
            value: "final".to_string(),
        });

        let mut traces = BTreeMap::new();

        // Single source - not overridden
        traces.insert(
            "single".to_string(),
            ValueTrace::new(vec![TracedValue::new(
                Value::String("only".to_string()),
                SourceLocation::new("defaults"),
                false,
            )])
            .unwrap(),
        );

        // Multiple sources - overridden
        traces.insert(
            "overridden".to_string(),
            ValueTrace::new(vec![
                TracedValue::new(
                    Value::String("first".to_string()),
                    SourceLocation::new("defaults"),
                    false,
                ),
                TracedValue::new(
                    Value::String("second".to_string()),
                    SourceLocation::new("config.toml"),
                    false,
                ),
            ])
            .unwrap(),
        );

        let traced = TracedConfig::new(config, traces);

        assert!(!traced.was_overridden("single"));
        assert!(traced.was_overridden("overridden"));
        assert!(!traced.was_overridden("nonexistent"));
    }

    #[test]
    fn test_traced_config_overridden_paths() {
        #[derive(Debug)]
        struct TestConfig;

        let config = Config::new(TestConfig);

        let mut traces = BTreeMap::new();

        traces.insert(
            "a".to_string(),
            ValueTrace::new(vec![TracedValue::new(
                Value::Integer(1),
                SourceLocation::new("defaults"),
                false,
            )])
            .unwrap(),
        );

        traces.insert(
            "b".to_string(),
            ValueTrace::new(vec![
                TracedValue::new(Value::Integer(1), SourceLocation::new("defaults"), false),
                TracedValue::new(Value::Integer(2), SourceLocation::new("file"), false),
            ])
            .unwrap(),
        );

        traces.insert(
            "c".to_string(),
            ValueTrace::new(vec![
                TracedValue::new(Value::Integer(1), SourceLocation::new("defaults"), false),
                TracedValue::new(Value::Integer(2), SourceLocation::new("file"), false),
                TracedValue::new(Value::Integer(3), SourceLocation::new("env"), false),
            ])
            .unwrap(),
        );

        let traced = TracedConfig::new(config, traces);

        let overridden: Vec<&str> = traced.overridden_paths().collect();
        assert_eq!(overridden.len(), 2);
        assert!(overridden.contains(&"b"));
        assert!(overridden.contains(&"c"));
    }

    #[test]
    fn test_traced_config_trace_report() {
        #[derive(Debug)]
        struct TestConfig;

        let config = Config::new(TestConfig);

        let mut traces = BTreeMap::new();

        traces.insert(
            "database.host".to_string(),
            ValueTrace::new(vec![
                TracedValue::new(
                    Value::String("localhost".to_string()),
                    SourceLocation::new("defaults"),
                    false,
                ),
                TracedValue::new(
                    Value::String("prod-db".to_string()),
                    SourceLocation::new("env:DB_HOST"),
                    false,
                ),
            ])
            .unwrap(),
        );

        traces.insert(
            "database.port".to_string(),
            ValueTrace::new(vec![TracedValue::new(
                Value::Integer(5432),
                SourceLocation::new("config.toml"),
                false,
            )])
            .unwrap(),
        );

        let traced = TracedConfig::new(config, traces);
        let report = traced.trace_report();

        assert!(report.contains("database.host"));
        assert!(report.contains("prod-db"));
        assert!(report.contains("<- overridden"));
        assert!(report.contains("database.port"));
        assert!(report.contains("5432"));
        assert!(report.contains("✓"));
        assert!(report.contains("○"));
    }

    #[test]
    fn test_trace_builder() {
        let mut builder = TraceBuilder::new();

        builder.add_value(
            "host".to_string(),
            Value::String("localhost".to_string()),
            SourceLocation::new("defaults"),
        );
        builder.add_value(
            "host".to_string(),
            Value::String("prod".to_string()),
            SourceLocation::new("env"),
        );
        builder.add_value(
            "port".to_string(),
            Value::Integer(8080),
            SourceLocation::new("defaults"),
        );

        let traces = builder.build();

        assert_eq!(traces.len(), 2);
        assert!(traces.get("host").unwrap().was_overridden());
        assert!(!traces.get("port").unwrap().was_overridden());
    }

    #[test]
    fn test_traced_config_deref() {
        #[derive(Debug)]
        struct TestConfig {
            value: i32,
        }

        let config = Config::new(TestConfig { value: 42 });
        let traced = TracedConfig::new(config, BTreeMap::new());

        // Test Deref
        assert_eq!(traced.value, 42);

        // Test AsRef
        let r: &TestConfig = traced.as_ref();
        assert_eq!(r.value, 42);
    }

    #[test]
    fn test_traced_config_into_inner() {
        #[derive(Debug, PartialEq)]
        struct TestConfig {
            value: i32,
        }

        let config = Config::new(TestConfig { value: 42 });
        let traced = TracedConfig::new(config, BTreeMap::new());

        let inner = traced.into_inner();
        assert_eq!(inner, TestConfig { value: 42 });
    }

    #[test]
    fn test_traced_config_into_config() {
        #[derive(Debug)]
        struct TestConfig {
            value: i32,
        }

        let config = Config::new(TestConfig { value: 42 });
        let traced = TracedConfig::new(config, BTreeMap::new());

        let config = traced.into_config();
        assert_eq!(config.get().value, 42);
    }

    #[test]
    fn test_traced_config_paths() {
        #[derive(Debug)]
        struct TestConfig;

        let config = Config::new(TestConfig);

        let mut traces = BTreeMap::new();
        traces.insert(
            "a".to_string(),
            ValueTrace::new(vec![TracedValue::new(
                Value::Integer(1),
                SourceLocation::new("test"),
                false,
            )])
            .unwrap(),
        );
        traces.insert(
            "b".to_string(),
            ValueTrace::new(vec![TracedValue::new(
                Value::Integer(2),
                SourceLocation::new("test"),
                false,
            )])
            .unwrap(),
        );

        let traced = TracedConfig::new(config, traces);

        let paths: Vec<&str> = traced.paths().collect();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&"a"));
        assert!(paths.contains(&"b"));
    }
}
