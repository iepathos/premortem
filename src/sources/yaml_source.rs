//! YAML configuration source.
//!
//! This module provides the `Yaml` source for loading configuration from YAML files
//! or strings. It supports required/optional files, string content, and rich error
//! reporting with source locations.
//!
//! # Example
//!
//! ```ignore
//! use premortem::{Config, Yaml};
//!
//! // Load from file (required by default)
//! let config = Config::<AppConfig>::builder()
//!     .source(Yaml::file("config.yaml"))
//!     .build()?;
//!
//! // Load from optional file
//! let config = Config::<AppConfig>::builder()
//!     .source(Yaml::file("config.yaml").optional())
//!     .build()?;
//!
//! // Load from string
//! let config = Config::<AppConfig>::builder()
//!     .source(Yaml::string("host: localhost\nport: 8080"))
//!     .build()?;
//! ```

use std::path::PathBuf;

use crate::env::ConfigEnv;
use crate::error::{ConfigError, ConfigErrors, SourceErrorKind, SourceLocation};
use crate::source::{ConfigValues, Source};
use crate::sources::line_from_offset;
use crate::value::{ConfigValue, Value};

/// The source type for YAML configuration.
#[derive(Debug, Clone)]
enum YamlSource {
    /// Load from a file path
    File(PathBuf),
    /// Load from a string
    String { content: String, name: String },
}

/// YAML configuration source.
///
/// Loads configuration from YAML files or strings with support for
/// required/optional files and rich error reporting.
#[derive(Debug, Clone)]
pub struct Yaml {
    source: YamlSource,
    required: bool,
    name: Option<String>,
}

impl Yaml {
    /// Load YAML from a file path (required by default).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Yaml;
    ///
    /// let source = Yaml::file("config.yaml");
    /// let source = Yaml::file("/etc/myapp/config.yaml");
    /// ```
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            source: YamlSource::File(path.into()),
            required: true,
            name: None,
        }
    }

    /// Load YAML from a string.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Yaml;
    ///
    /// let source = Yaml::string("host: localhost\nport: 8080");
    /// ```
    pub fn string(content: impl Into<String>) -> Self {
        Self {
            source: YamlSource::String {
                content: content.into(),
                name: "<string>".to_string(),
            },
            required: true,
            name: None,
        }
    }

    /// Mark this source as optional (no error if file missing).
    ///
    /// When a file is marked as optional, a missing file will result in
    /// empty configuration values instead of an error.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Yaml;
    ///
    /// let source = Yaml::file("config.yaml").optional();
    /// ```
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// Mark this source as required (default).
    ///
    /// This is the default behavior - a missing file will result in an error.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Yaml;
    ///
    /// let source = Yaml::file("config.yaml").required();
    /// ```
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set a custom name for this source in error messages.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Yaml;
    ///
    /// let source = Yaml::file("config.yaml").named("base configuration");
    /// ```
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Get the source name for error messages.
    fn source_name(&self) -> String {
        if let Some(ref name) = self.name {
            return name.clone();
        }

        match &self.source {
            YamlSource::File(path) => path.display().to_string(),
            YamlSource::String { name, .. } => name.clone(),
        }
    }
}

impl Source for Yaml {
    /// Load YAML configuration.
    ///
    /// File I/O is performed through the `ConfigEnv` trait, enabling
    /// dependency injection for testing. Parsing is pure and happens
    /// after the I/O completes.
    fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
        let source_name = self.source_name();

        let content = match &self.source {
            YamlSource::File(path) => match env.read_file(path) {
                Ok(content) => content,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if self.required {
                        return Err(ConfigErrors::single(ConfigError::SourceError {
                            source_name,
                            kind: SourceErrorKind::NotFound {
                                path: path.display().to_string(),
                            },
                        }));
                    } else {
                        // Optional file missing = empty values (success)
                        return Ok(ConfigValues::empty());
                    }
                }
                Err(e) => {
                    return Err(ConfigErrors::single(ConfigError::SourceError {
                        source_name,
                        kind: SourceErrorKind::IoError {
                            message: e.to_string(),
                        },
                    }));
                }
            },
            YamlSource::String { content, .. } => content.clone(),
        };

        // Pure parsing (after I/O)
        parse_yaml(&content, &source_name)
    }

    fn name(&self) -> &str {
        match &self.name {
            Some(name) => name,
            None => match &self.source {
                YamlSource::File(path) => path.to_str().unwrap_or("<file>"),
                YamlSource::String { name, .. } => name,
            },
        }
    }

    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        match &self.source {
            YamlSource::File(path) => Some(path.clone()),
            YamlSource::String { .. } => None,
        }
    }

    #[cfg(feature = "watch")]
    fn clone_box(&self) -> Box<dyn Source> {
        Box::new(self.clone())
    }
}

/// Pure function: parse YAML content into ConfigValues.
/// No I/O - this runs after the Effect has read the file.
/// Tracks line numbers by searching for key positions in the content.
fn parse_yaml(content: &str, source_name: &str) -> Result<ConfigValues, ConfigErrors> {
    let document: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e: serde_yaml::Error| {
            // serde_yaml errors include location info
            let (line, column) = e.location().map_or((None, None), |loc| {
                (Some(loc.line() as u32), Some(loc.column() as u32))
            });
            ConfigErrors::single(ConfigError::SourceError {
                source_name: source_name.to_string(),
                kind: SourceErrorKind::ParseError {
                    message: e.to_string(),
                    line,
                    column,
                },
            })
        })?;

    // Pure transformation: YAML -> ConfigValues with line tracking
    let mut values = ConfigValues::empty();
    flatten_value_with_lines(&document, "", source_name, content, &mut values);
    Ok(values)
}

/// Pure function: recursively flatten YAML structure to dot-notation paths with line tracking.
fn flatten_value_with_lines(
    value: &serde_yaml::Value,
    prefix: &str,
    source_name: &str,
    content: &str,
    values: &mut ConfigValues,
) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            for (key, val) in map {
                let key_str = match key {
                    serde_yaml::Value::String(s) => s.clone(),
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::Bool(b) => b.to_string(),
                    _ => continue, // Skip non-string/number/bool keys
                };

                let path = if prefix.is_empty() {
                    key_str
                } else {
                    format!("{}.{}", prefix, key_str)
                };
                flatten_value_with_lines(val, &path, source_name, content, values);
            }
        }
        serde_yaml::Value::Sequence(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let path = format!("{}[{}]", prefix, i);
                flatten_value_with_lines(val, &path, source_name, content, values);
            }
            // Store array length with location from parent key
            let line = find_key_line(content, prefix);
            let mut loc = SourceLocation::new(source_name);
            if let Some(l) = line {
                loc = loc.with_line(l);
            }
            values.insert(
                format!("{}.__len", prefix),
                ConfigValue::new(Value::Integer(arr.len() as i64), loc),
            );
        }
        serde_yaml::Value::Tagged(tagged) => {
            // Handle tagged values by extracting the inner value
            flatten_value_with_lines(&tagged.value, prefix, source_name, content, values);
        }
        _ => {
            // Find the line where this key appears
            let line = find_key_line(content, prefix);
            let mut loc = SourceLocation::new(source_name);
            if let Some(l) = line {
                loc = loc.with_line(l);
            }
            values.insert(
                prefix.to_string(),
                ConfigValue::new(yaml_to_value(value), loc),
            );
        }
    }
}

/// Find the line where a key appears in YAML content.
/// Searches for the last segment of a dotted path (e.g., "host" in "database.host").
fn find_key_line(content: &str, path: &str) -> Option<u32> {
    // Get the last segment of the path (the actual key name in YAML)
    let key = path.split('.').next_back().unwrap_or(path);
    // Strip array index if present
    let key = key.split('[').next().unwrap_or(key);

    // Search for "key:" pattern (YAML key syntax)
    let pattern = format!("{}:", key);

    // Find the first occurrence of the key pattern
    content
        .find(&pattern)
        .map(|offset| line_from_offset(content, offset))
}

/// Convert a serde_yaml::Value to our Value type.
fn yaml_to_value(yaml: &serde_yaml::Value) -> Value {
    match yaml {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(*b),
        serde_yaml::Value::Number(n) => {
            // Try integer first, fall back to float
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                // This shouldn't happen with serde_yaml, but handle it gracefully
                Value::String(n.to_string())
            }
        }
        serde_yaml::Value::String(s) => Value::String(s.clone()),
        serde_yaml::Value::Sequence(arr) => Value::Array(arr.iter().map(yaml_to_value).collect()),
        serde_yaml::Value::Mapping(map) => Value::Table(
            map.iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s.clone(),
                        serde_yaml::Value::Number(n) => n.to_string(),
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        _ => return None,
                    };
                    Some((key, yaml_to_value(v)))
                })
                .collect(),
        ),
        serde_yaml::Value::Tagged(tagged) => yaml_to_value(&tagged.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockEnv;

    #[test]
    fn test_yaml_file_load() {
        let env = MockEnv::new().with_file("config.yaml", "host: localhost\nport: 8080");

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert_eq!(
            values.get("port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
    }

    #[test]
    fn test_yaml_file_missing_required() {
        let env = MockEnv::new();

        let source = Yaml::file("missing.yaml");
        let result = source.load(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        match errors.first() {
            ConfigError::SourceError { kind, .. } => {
                assert!(matches!(kind, SourceErrorKind::NotFound { .. }));
            }
            _ => panic!("Expected SourceError"),
        }
    }

    #[test]
    fn test_yaml_file_missing_optional() {
        let env = MockEnv::new();

        let source = Yaml::file("missing.yaml").optional();
        let values = source.load(&env).expect("should succeed with empty values");

        assert!(values.is_empty());
    }

    #[test]
    fn test_yaml_file_permission_denied() {
        let env = MockEnv::new().with_unreadable_file("secret.yaml");

        let source = Yaml::file("secret.yaml");
        let result = source.load(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        match errors.first() {
            ConfigError::SourceError { kind, .. } => {
                assert!(matches!(kind, SourceErrorKind::IoError { .. }));
            }
            _ => panic!("Expected SourceError"),
        }
    }

    #[test]
    fn test_yaml_string_load() {
        let env = MockEnv::new();

        let source = Yaml::string("host: localhost\nport: 8080");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert_eq!(
            values.get("port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
    }

    #[test]
    fn test_yaml_nested_objects() {
        let env = MockEnv::new().with_file(
            "config.yaml",
            r#"
database:
  host: localhost
  port: 5432
  pool:
    min_size: 5
    max_size: 20
"#,
        );

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("database.host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert_eq!(
            values.get("database.port").map(|v| v.value.as_integer()),
            Some(Some(5432))
        );
        assert_eq!(
            values
                .get("database.pool.min_size")
                .map(|v| v.value.as_integer()),
            Some(Some(5))
        );
        assert_eq!(
            values
                .get("database.pool.max_size")
                .map(|v| v.value.as_integer()),
            Some(Some(20))
        );
    }

    #[test]
    fn test_yaml_arrays() {
        let env = MockEnv::new().with_file(
            "config.yaml",
            r#"
hosts:
  - host1
  - host2
  - host3
"#,
        );

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("hosts[0]").map(|v| v.value.as_str()),
            Some(Some("host1"))
        );
        assert_eq!(
            values.get("hosts[1]").map(|v| v.value.as_str()),
            Some(Some("host2"))
        );
        assert_eq!(
            values.get("hosts[2]").map(|v| v.value.as_str()),
            Some(Some("host3"))
        );
        assert_eq!(
            values.get("hosts.__len").map(|v| v.value.as_integer()),
            Some(Some(3))
        );
    }

    #[test]
    fn test_yaml_all_value_types() {
        let env = MockEnv::new().with_file(
            "config.yaml",
            r#"
string_val: hello
int_val: 42
float_val: 2.72
bool_val: true
null_val: null
"#,
        );

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("string_val").map(|v| v.value.as_str()),
            Some(Some("hello"))
        );
        assert_eq!(
            values.get("int_val").map(|v| v.value.as_integer()),
            Some(Some(42))
        );
        assert_eq!(
            values.get("float_val").map(|v| v.value.as_float()),
            Some(Some(2.72))
        );
        assert_eq!(
            values.get("bool_val").map(|v| v.value.as_bool()),
            Some(Some(true))
        );
        // Null value should be present
        assert!(values.get("null_val").is_some());
        assert!(values.get("null_val").unwrap().value.is_null());
    }

    #[test]
    fn test_yaml_parse_error_with_location() {
        let env = MockEnv::new().with_file("config.yaml", "host: localhost\nport: [invalid");

        let source = Yaml::file("config.yaml");
        let result = source.load(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        match errors.first() {
            ConfigError::SourceError { kind, .. } => {
                assert!(matches!(kind, SourceErrorKind::ParseError { .. }));
            }
            _ => panic!("Expected SourceError"),
        }
    }

    #[test]
    fn test_yaml_custom_name() {
        let env = MockEnv::new().with_file("config.yaml", "host: localhost");

        let source = Yaml::file("config.yaml").named("production config");
        assert_eq!(source.name(), "production config");

        let values = source.load(&env).expect("should load successfully");
        assert_eq!(
            values.get("host").unwrap().source.source,
            "production config"
        );
    }

    #[test]
    fn test_yaml_required_method() {
        let source = Yaml::file("config.yaml").optional().required();
        let env = MockEnv::new();
        let result = source.load(&env);

        // Should fail because file is missing and source is required
        assert!(result.is_err());
    }

    #[test]
    fn test_yaml_source_location_tracking() {
        let env = MockEnv::new().with_file("config.yaml", "host: localhost");

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        let host_value = values.get("host").expect("host should exist");
        assert_eq!(host_value.source.source, "config.yaml");
    }

    #[test]
    fn test_yaml_array_of_objects() {
        let env = MockEnv::new().with_file(
            "config.yaml",
            r#"
servers:
  - name: server1
    port: 8080
  - name: server2
    port: 8081
"#,
        );

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("servers[0].name").map(|v| v.value.as_str()),
            Some(Some("server1"))
        );
        assert_eq!(
            values.get("servers[0].port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
        assert_eq!(
            values.get("servers[1].name").map(|v| v.value.as_str()),
            Some(Some("server2"))
        );
        assert_eq!(
            values.get("servers[1].port").map(|v| v.value.as_integer()),
            Some(Some(8081))
        );
        assert_eq!(
            values.get("servers.__len").map(|v| v.value.as_integer()),
            Some(Some(2))
        );
    }

    #[test]
    fn test_yaml_large_integer() {
        let env = MockEnv::new().with_file("config.yaml", "large_int: 9007199254740992");

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("large_int").map(|v| v.value.as_integer()),
            Some(Some(9007199254740992))
        );
    }

    #[test]
    fn test_yaml_negative_numbers() {
        let env =
            MockEnv::new().with_file("config.yaml", "negative_int: -42\nnegative_float: -2.75");

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("negative_int").map(|v| v.value.as_integer()),
            Some(Some(-42))
        );
        assert_eq!(
            values.get("negative_float").map(|v| v.value.as_float()),
            Some(Some(-2.75))
        );
    }

    #[test]
    fn test_yaml_line_number_tracking() {
        let env =
            MockEnv::new().with_file("config.yaml", "host: localhost\nport: 8080\ndebug: true");

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        // host is on line 1
        let host_value = values.get("host").expect("host should exist");
        assert_eq!(host_value.source.source, "config.yaml");
        assert_eq!(host_value.source.line, Some(1));

        // port is on line 2
        let port_value = values.get("port").expect("port should exist");
        assert_eq!(port_value.source.line, Some(2));

        // debug is on line 3
        let debug_value = values.get("debug").expect("debug should exist");
        assert_eq!(debug_value.source.line, Some(3));
    }

    #[test]
    fn test_yaml_nested_object_line_tracking() {
        let env =
            MockEnv::new().with_file("config.yaml", "database:\n  host: localhost\n  port: 5432");

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        // database.host is on line 2
        let host_value = values.get("database.host").expect("host should exist");
        assert_eq!(host_value.source.line, Some(2));

        // database.port is on line 3
        let port_value = values.get("database.port").expect("port should exist");
        assert_eq!(port_value.source.line, Some(3));
    }

    #[test]
    fn test_yaml_anchors_and_aliases() {
        // Test simple anchor and alias resolution (not merge keys)
        // serde_yaml resolves aliases automatically during parsing
        let env = MockEnv::new().with_file(
            "config.yaml",
            r#"
default_timeout: &timeout 30

database:
  timeout: *timeout
  host: localhost
"#,
        );

        let source = Yaml::file("config.yaml");
        let values = source.load(&env).expect("should load successfully");

        // Alias value should be resolved to the anchor value
        assert_eq!(
            values.get("database.timeout").map(|v| v.value.as_integer()),
            Some(Some(30))
        );
        assert_eq!(
            values.get("database.host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        // The anchor source is also available
        assert_eq!(
            values.get("default_timeout").map(|v| v.value.as_integer()),
            Some(Some(30))
        );
    }

    #[test]
    fn test_yaml_optional_method() {
        let source = Yaml::file("config.yaml").optional();
        let env = MockEnv::new();
        let result = source.load(&env);

        // Should succeed with empty values
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
