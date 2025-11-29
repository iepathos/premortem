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
    ///
    /// Also handles array notation like `hosts[0]` -> `{"hosts": ["..."]}`
    pub fn to_json(&self) -> serde_json::Value {
        let mut root = serde_json::Value::Object(serde_json::Map::new());

        // First pass: handle empty arrays using __len metadata
        // Empty arrays have no [n] elements, so we need __len to know they exist
        for (path, config_value) in self.iter() {
            if path.ends_with(".__len") {
                if let Some(0) = config_value.value.as_integer() {
                    // This is an empty array - create it
                    let array_path = &path[..path.len() - 6]; // Remove ".__len"
                    let segments = parse_path(array_path);
                    insert_value(&mut root, &segments, serde_json::Value::Array(Vec::new()));
                }
            }
        }

        // Second pass: insert all actual values
        for (path, config_value) in self.iter() {
            // Skip internal metadata keys (e.g., "hosts.__len")
            if path.contains(".__") {
                continue;
            }

            let segments = parse_path(path);
            insert_value(&mut root, &segments, value_to_json(&config_value.value));
        }

        root
    }
}

/// A segment in a configuration path.
#[derive(Debug, Clone, PartialEq)]
enum PathSegment {
    /// Object key like "database" in "database.host"
    Key(String),
    /// Array index like 0 in "hosts\[0\]"
    Index(usize),
}

/// Parse a path string into segments.
///
/// Handles paths like:
/// - "database.host" -> \[Key("database"), Key("host")\]
/// - "hosts\[0\]" -> \[Key("hosts"), Index(0)\]
/// - "servers\[0\].host" -> \[Key("servers"), Index(0), Key("host")\]
/// - "matrix\[0\]\[1\]" -> \[Key("matrix"), Index(0), Index(1)\]
fn parse_path(path: &str) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    let mut current = String::new();

    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(current.clone()));
                    current.clear();
                }
            }
            '[' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(current.clone()));
                    current.clear();
                }
                // Parse the index
                let mut index_str = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == ']' {
                        chars.next(); // consume ']'
                        break;
                    }
                    index_str.push(chars.next().unwrap());
                }
                if let Ok(index) = index_str.parse::<usize>() {
                    segments.push(PathSegment::Index(index));
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        segments.push(PathSegment::Key(current));
    }

    segments
}

/// Insert a value into a JSON structure following the path segments.
fn insert_value(root: &mut serde_json::Value, segments: &[PathSegment], value: serde_json::Value) {
    if segments.is_empty() {
        *root = value;
        return;
    }

    match &segments[0] {
        PathSegment::Key(key) => {
            // Ensure root is an object
            if !root.is_object() {
                *root = serde_json::Value::Object(serde_json::Map::new());
            }

            let obj = root.as_object_mut().unwrap();

            if segments.len() == 1 {
                obj.insert(key.clone(), value);
            } else {
                // Peek at next segment to determine child type
                let child = obj
                    .entry(key.clone())
                    .or_insert_with(|| match &segments[1] {
                        PathSegment::Index(_) => serde_json::Value::Array(Vec::new()),
                        PathSegment::Key(_) => serde_json::Value::Object(serde_json::Map::new()),
                    });
                insert_value(child, &segments[1..], value);
            }
        }
        PathSegment::Index(index) => {
            // Ensure root is an array
            if !root.is_array() {
                *root = serde_json::Value::Array(Vec::new());
            }

            let arr = root.as_array_mut().unwrap();

            // Extend array if needed
            while arr.len() <= *index {
                arr.push(serde_json::Value::Null);
            }

            if segments.len() == 1 {
                arr[*index] = value;
            } else {
                // Peek at next segment to determine child type
                if arr[*index].is_null() {
                    arr[*index] = match &segments[1] {
                        PathSegment::Index(_) => serde_json::Value::Array(Vec::new()),
                        PathSegment::Key(_) => serde_json::Value::Object(serde_json::Map::new()),
                    };
                }
                insert_value(&mut arr[*index], &segments[1..], value);
            }
        }
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

    #[test]
    fn test_parse_path_simple() {
        let segments = parse_path("host");
        assert_eq!(segments, vec![PathSegment::Key("host".to_string())]);
    }

    #[test]
    fn test_parse_path_nested() {
        let segments = parse_path("database.host");
        assert_eq!(
            segments,
            vec![
                PathSegment::Key("database".to_string()),
                PathSegment::Key("host".to_string())
            ]
        );
    }

    #[test]
    fn test_parse_path_array_index() {
        let segments = parse_path("hosts[0]");
        assert_eq!(
            segments,
            vec![PathSegment::Key("hosts".to_string()), PathSegment::Index(0)]
        );
    }

    #[test]
    fn test_parse_path_array_with_nested() {
        let segments = parse_path("servers[0].host");
        assert_eq!(
            segments,
            vec![
                PathSegment::Key("servers".to_string()),
                PathSegment::Index(0),
                PathSegment::Key("host".to_string())
            ]
        );
    }

    #[test]
    fn test_parse_path_nested_arrays() {
        let segments = parse_path("matrix[0][1]");
        assert_eq!(
            segments,
            vec![
                PathSegment::Key("matrix".to_string()),
                PathSegment::Index(0),
                PathSegment::Index(1)
            ]
        );
    }

    #[test]
    fn test_to_json_with_simple_array() {
        let mut values = ConfigValues::empty();
        values.insert(
            "hosts[0]".to_string(),
            ConfigValue::new(
                Value::String("host1".to_string()),
                SourceLocation::new("test"),
            ),
        );
        values.insert(
            "hosts[1]".to_string(),
            ConfigValue::new(
                Value::String("host2".to_string()),
                SourceLocation::new("test"),
            ),
        );

        let json = values.to_json();

        assert!(json["hosts"].is_array());
        let arr = json["hosts"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], "host1");
        assert_eq!(arr[1], "host2");
    }

    #[test]
    fn test_to_json_with_array_of_objects() {
        let mut values = ConfigValues::empty();
        values.insert(
            "servers[0].host".to_string(),
            ConfigValue::new(
                Value::String("server1".to_string()),
                SourceLocation::new("test"),
            ),
        );
        values.insert(
            "servers[0].port".to_string(),
            ConfigValue::new(Value::Integer(8080), SourceLocation::new("test")),
        );
        values.insert(
            "servers[1].host".to_string(),
            ConfigValue::new(
                Value::String("server2".to_string()),
                SourceLocation::new("test"),
            ),
        );
        values.insert(
            "servers[1].port".to_string(),
            ConfigValue::new(Value::Integer(8081), SourceLocation::new("test")),
        );

        let json = values.to_json();

        assert!(json["servers"].is_array());
        let arr = json["servers"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["host"], "server1");
        assert_eq!(arr[0]["port"], 8080);
        assert_eq!(arr[1]["host"], "server2");
        assert_eq!(arr[1]["port"], 8081);
    }

    #[test]
    fn test_to_json_skips_len_metadata() {
        let mut values = ConfigValues::empty();
        values.insert(
            "hosts[0]".to_string(),
            ConfigValue::new(
                Value::String("host1".to_string()),
                SourceLocation::new("test"),
            ),
        );
        values.insert(
            "hosts.__len".to_string(),
            ConfigValue::new(Value::Integer(1), SourceLocation::new("test")),
        );

        let json = values.to_json();

        assert!(json["hosts"].is_array());
        // __len should not appear in output
        assert!(json["hosts"]["__len"].is_null());
    }

    #[test]
    fn test_to_json_mixed_object_and_array() {
        let mut values = ConfigValues::empty();
        values.insert(
            "config.name".to_string(),
            ConfigValue::new(
                Value::String("myapp".to_string()),
                SourceLocation::new("test"),
            ),
        );
        values.insert(
            "config.hosts[0]".to_string(),
            ConfigValue::new(
                Value::String("localhost".to_string()),
                SourceLocation::new("test"),
            ),
        );
        values.insert(
            "config.hosts[1]".to_string(),
            ConfigValue::new(
                Value::String("remote".to_string()),
                SourceLocation::new("test"),
            ),
        );

        let json = values.to_json();

        assert_eq!(json["config"]["name"], "myapp");
        assert!(json["config"]["hosts"].is_array());
        let arr = json["config"]["hosts"].as_array().unwrap();
        assert_eq!(arr[0], "localhost");
        assert_eq!(arr[1], "remote");
    }

    #[test]
    fn test_to_json_empty_array_from_len_metadata() {
        let mut values = ConfigValues::empty();
        values.insert(
            "name".to_string(),
            ConfigValue::new(
                Value::String("test".to_string()),
                SourceLocation::new("test"),
            ),
        );
        // Empty array represented only by __len = 0
        values.insert(
            "hosts.__len".to_string(),
            ConfigValue::new(Value::Integer(0), SourceLocation::new("test")),
        );

        let json = values.to_json();

        assert_eq!(json["name"], "test");
        assert!(json["hosts"].is_array());
        let arr = json["hosts"].as_array().unwrap();
        assert!(arr.is_empty());
    }

    #[test]
    fn test_to_json_nested_empty_array() {
        let mut values = ConfigValues::empty();
        values.insert(
            "config.name".to_string(),
            ConfigValue::new(
                Value::String("test".to_string()),
                SourceLocation::new("test"),
            ),
        );
        values.insert(
            "config.items.__len".to_string(),
            ConfigValue::new(Value::Integer(0), SourceLocation::new("test")),
        );

        let json = values.to_json();

        assert_eq!(json["config"]["name"], "test");
        assert!(json["config"]["items"].is_array());
        assert!(json["config"]["items"].as_array().unwrap().is_empty());
    }
}
