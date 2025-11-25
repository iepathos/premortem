//! Configuration source trait and types.
//!
//! This module provides the `Source` trait for loading configuration from
//! various sources, and `ConfigValues` for intermediate value storage.

use std::collections::BTreeMap;
#[cfg(feature = "watch")]
use std::path::PathBuf;

use crate::env::ConfigEnv;
use crate::error::ConfigErrors;
use crate::value::ConfigValue;

/// Intermediate representation of configuration values.
///
/// This is the "data" that flows through the pure core. Each source
/// produces `ConfigValues`, which are then merged together before
/// deserialization.
#[derive(Debug, Clone, Default)]
pub struct ConfigValues {
    /// Values stored by their dot-notation path (e.g., "database.host")
    values: BTreeMap<String, ConfigValue>,
}

impl ConfigValues {
    /// Create an empty ConfigValues container.
    pub fn empty() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Insert a value at the given path.
    pub fn insert(&mut self, path: String, value: ConfigValue) {
        self.values.insert(path, value);
    }

    /// Get a value by path.
    pub fn get(&self, path: &str) -> Option<&ConfigValue> {
        self.values.get(path)
    }

    /// Check if a path exists.
    pub fn contains(&self, path: &str) -> bool {
        self.values.contains_key(path)
    }

    /// Get the number of values.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterate over all path-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ConfigValue)> {
        self.values.iter()
    }

    /// Get all paths.
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.values.keys()
    }

    /// Convert the internal values to a nested structure for JSON serialization.
    ///
    /// Transforms flat paths like "database.host" into nested JSON:
    /// `{"database": {"host": "..."}}`
    pub fn to_json(&self) -> serde_json::Value {
        let mut root = serde_json::Map::new();

        for (path, config_value) in self.iter() {
            let parts: Vec<&str> = path.split('.').collect();
            insert_at_path(&mut root, &parts, value_to_json(&config_value.value));
        }

        serde_json::Value::Object(root)
    }
}

/// Insert a JSON value at a nested path.
fn insert_at_path(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    path: &[&str],
    value: serde_json::Value,
) {
    if path.is_empty() {
        return;
    }

    if path.len() == 1 {
        obj.insert(path[0].to_string(), value);
        return;
    }

    let key = path[0].to_string();
    let child = obj
        .entry(key)
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

    if let serde_json::Value::Object(ref mut map) = child {
        insert_at_path(map, &path[1..], value);
    }
}

/// Convert a Value to serde_json::Value.
fn value_to_json(value: &crate::value::Value) -> serde_json::Value {
    use crate::value::Value;

    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Integer(i) => serde_json::Value::Number((*i).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(value_to_json).collect()),
        Value::Table(table) => {
            let map: serde_json::Map<String, serde_json::Value> = table
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
    }
}

/// Trait for configuration sources.
///
/// Sources perform I/O through the `ConfigEnv` trait for testable dependency
/// injection. This keeps I/O at the boundaries (imperative shell).
///
/// # Example Implementation
///
/// ```ignore
/// impl Source for MySource {
///     fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
///         // I/O through injected environment
///         let content = env.read_file(&self.path)
///             .map_err(|e| ConfigErrors::single(ConfigError::SourceError {
///                 source_name: self.path.display().to_string(),
///                 kind: SourceErrorKind::IoError { message: e.to_string() },
///             }))?;
///
///         // Pure parsing
///         parse_content(&content)
///     }
///
///     fn name(&self) -> &str {
///         "my-source"
///     }
/// }
/// ```
pub trait Source: Send + Sync {
    /// Load configuration values from this source.
    ///
    /// The `ConfigEnv` parameter enables dependency injection for testing.
    /// In production, use `RealEnv`; in tests, use `MockEnv`.
    fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors>;

    /// Human-readable name of this source for error messages.
    fn name(&self) -> &str;

    /// Path to watch for hot reload, if applicable.
    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        None
    }

    /// Clone this source into a boxed trait object.
    ///
    /// Required for hot reload to rebuild configuration from the same sources.
    #[cfg(feature = "watch")]
    fn clone_box(&self) -> Box<dyn Source>;
}

/// Pure function: merge multiple ConfigValues by priority.
///
/// Later values override earlier values. This is the core merge logic
/// that combines configuration from multiple sources.
pub fn merge_config_values(all_values: Vec<ConfigValues>) -> ConfigValues {
    let mut merged = ConfigValues::empty();

    for values in all_values {
        for (path, value) in values.values {
            merged.values.insert(path, value);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SourceLocation;
    use crate::value::Value;

    #[test]
    fn test_config_values_basic() {
        let mut values = ConfigValues::empty();
        assert!(values.is_empty());

        values.insert(
            "database.host".to_string(),
            ConfigValue::new(
                Value::String("localhost".to_string()),
                SourceLocation::new("test"),
            ),
        );

        assert_eq!(values.len(), 1);
        assert!(values.contains("database.host"));
        assert!(!values.contains("database.port"));

        let val = values.get("database.host").unwrap();
        assert_eq!(val.value.as_str(), Some("localhost"));
    }

    #[test]
    fn test_merge_config_values() {
        let mut v1 = ConfigValues::empty();
        v1.insert(
            "host".to_string(),
            ConfigValue::new(
                Value::String("localhost".to_string()),
                SourceLocation::new("file"),
            ),
        );
        v1.insert(
            "port".to_string(),
            ConfigValue::new(Value::Integer(8080), SourceLocation::new("file")),
        );

        let mut v2 = ConfigValues::empty();
        v2.insert(
            "host".to_string(),
            ConfigValue::new(
                Value::String("production".to_string()),
                SourceLocation::new("env"),
            ),
        );
        v2.insert(
            "debug".to_string(),
            ConfigValue::new(Value::Bool(true), SourceLocation::new("env")),
        );

        let merged = merge_config_values(vec![v1, v2]);

        // v2 should override v1 for "host"
        assert_eq!(
            merged.get("host").unwrap().value.as_str(),
            Some("production")
        );
        // "port" from v1 should remain
        assert_eq!(merged.get("port").unwrap().value.as_integer(), Some(8080));
        // "debug" from v2 should be added
        assert_eq!(merged.get("debug").unwrap().value.as_bool(), Some(true));
    }

    #[test]
    fn test_config_values_to_json() {
        let mut values = ConfigValues::empty();
        values.insert(
            "database.host".to_string(),
            ConfigValue::new(
                Value::String("localhost".to_string()),
                SourceLocation::new("test"),
            ),
        );
        values.insert(
            "database.port".to_string(),
            ConfigValue::new(Value::Integer(5432), SourceLocation::new("test")),
        );
        values.insert(
            "debug".to_string(),
            ConfigValue::new(Value::Bool(true), SourceLocation::new("test")),
        );

        let json = values.to_json();

        assert_eq!(json["database"]["host"], "localhost");
        assert_eq!(json["database"]["port"], 5432);
        assert_eq!(json["debug"], true);
    }
}
