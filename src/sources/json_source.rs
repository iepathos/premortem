//! JSON configuration source.
//!
//! This module provides the `Json` source for loading configuration from JSON files
//! or strings. It supports required/optional files, string content, and rich error
//! reporting with source locations.
//!
//! # Example
//!
//! ```ignore
//! use premortem::{Config, Json};
//!
//! // Load from file (required by default)
//! let config = Config::<AppConfig>::builder()
//!     .source(Json::file("config.json"))
//!     .build()?;
//!
//! // Load from optional file
//! let config = Config::<AppConfig>::builder()
//!     .source(Json::file("config.json").optional())
//!     .build()?;
//!
//! // Load from string
//! let config = Config::<AppConfig>::builder()
//!     .source(Json::string(r#"{"host": "localhost", "port": 8080}"#))
//!     .build()?;
//! ```

use std::path::PathBuf;

use crate::env::ConfigEnv;
use crate::error::{ConfigError, ConfigErrors, SourceErrorKind, SourceLocation};
use crate::source::{ConfigValues, Source};
use crate::value::{ConfigValue, Value};

/// The source type for JSON configuration.
#[derive(Debug, Clone)]
enum JsonSource {
    /// Load from a file path
    File(PathBuf),
    /// Load from a string
    String { content: String, name: String },
}

/// JSON configuration source.
///
/// Loads configuration from JSON files or strings with support for
/// required/optional files and rich error reporting.
#[derive(Debug, Clone)]
pub struct Json {
    source: JsonSource,
    required: bool,
    name: Option<String>,
}

impl Json {
    /// Load JSON from a file path (required by default).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Json;
    ///
    /// let source = Json::file("config.json");
    /// let source = Json::file("/etc/myapp/config.json");
    /// ```
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            source: JsonSource::File(path.into()),
            required: true,
            name: None,
        }
    }

    /// Load JSON from a string.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Json;
    ///
    /// let source = Json::string(r#"{"host": "localhost", "port": 8080}"#);
    /// ```
    pub fn string(content: impl Into<String>) -> Self {
        Self {
            source: JsonSource::String {
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
    /// use premortem::Json;
    ///
    /// let source = Json::file("config.json").optional();
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
    /// use premortem::Json;
    ///
    /// let source = Json::file("config.json").required();
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
    /// use premortem::Json;
    ///
    /// let source = Json::file("config.json").named("base configuration");
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
            JsonSource::File(path) => path.display().to_string(),
            JsonSource::String { name, .. } => name.clone(),
        }
    }
}

impl Source for Json {
    /// Load JSON configuration.
    ///
    /// File I/O is performed through the `ConfigEnv` trait, enabling
    /// dependency injection for testing. Parsing is pure and happens
    /// after the I/O completes.
    fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
        let source_name = self.source_name();

        let content = match &self.source {
            JsonSource::File(path) => match env.read_file(path) {
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
            JsonSource::String { content, .. } => content.clone(),
        };

        // Pure parsing (after I/O)
        parse_json(&content, &source_name)
    }

    fn name(&self) -> &str {
        match &self.name {
            Some(name) => name,
            None => match &self.source {
                JsonSource::File(path) => path.to_str().unwrap_or("<file>"),
                JsonSource::String { name, .. } => name,
            },
        }
    }

    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        match &self.source {
            JsonSource::File(path) => Some(path.clone()),
            JsonSource::String { .. } => None,
        }
    }

    #[cfg(feature = "watch")]
    fn clone_box(&self) -> Box<dyn Source> {
        Box::new(self.clone())
    }
}

/// Pure function: parse JSON content into ConfigValues.
/// No I/O - this runs after the Effect has read the file.
fn parse_json(content: &str, source_name: &str) -> Result<ConfigValues, ConfigErrors> {
    let document: serde_json::Value =
        serde_json::from_str(content).map_err(|e: serde_json::Error| {
            // serde_json errors include line/column
            ConfigErrors::single(ConfigError::SourceError {
                source_name: source_name.to_string(),
                kind: SourceErrorKind::ParseError {
                    message: e.to_string(),
                    line: Some(e.line() as u32),
                    column: Some(e.column() as u32),
                },
            })
        })?;

    // Pure transformation: JSON -> ConfigValues
    let mut values = ConfigValues::empty();
    flatten_value(&document, "", source_name, &mut values);
    Ok(values)
}

/// Pure function: recursively flatten JSON structure to dot-notation paths.
fn flatten_value(
    value: &serde_json::Value,
    prefix: &str,
    source_name: &str,
    values: &mut ConfigValues,
) {
    match value {
        serde_json::Value::Object(obj) => {
            for (key, val) in obj {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_value(val, &path, source_name, values);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let path = format!("{}[{}]", prefix, i);
                flatten_value(val, &path, source_name, values);
            }
            // Also store array length for validation
            values.insert(
                format!("{}.__len", prefix),
                ConfigValue::new(
                    Value::Integer(arr.len() as i64),
                    SourceLocation::new(source_name),
                ),
            );
        }
        _ => {
            let config_value =
                ConfigValue::new(json_to_value(value), SourceLocation::new(source_name));
            values.insert(prefix.to_string(), config_value);
        }
    }
}

/// Convert a serde_json::Value to our Value type.
fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            // Try integer first, fall back to float
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                // This shouldn't happen with serde_json, but handle it gracefully
                Value::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::Array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => Value::Table(
            obj.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockEnv;

    #[test]
    fn test_json_file_load() {
        let env = MockEnv::new().with_file("config.json", r#"{"host": "localhost", "port": 8080}"#);

        let source = Json::file("config.json");
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
    fn test_json_file_missing_required() {
        let env = MockEnv::new();

        let source = Json::file("missing.json");
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
    fn test_json_file_missing_optional() {
        let env = MockEnv::new();

        let source = Json::file("missing.json").optional();
        let values = source.load(&env).expect("should succeed with empty values");

        assert!(values.is_empty());
    }

    #[test]
    fn test_json_file_permission_denied() {
        let env = MockEnv::new().with_unreadable_file("secret.json");

        let source = Json::file("secret.json");
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
    fn test_json_string_load() {
        let env = MockEnv::new();

        let source = Json::string(r#"{"host": "localhost", "port": 8080}"#);
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
    fn test_json_nested_objects() {
        let env = MockEnv::new().with_file(
            "config.json",
            r#"{
                "database": {
                    "host": "localhost",
                    "port": 5432,
                    "pool": {
                        "min_size": 5,
                        "max_size": 20
                    }
                }
            }"#,
        );

        let source = Json::file("config.json");
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
    fn test_json_arrays() {
        let env =
            MockEnv::new().with_file("config.json", r#"{"hosts": ["host1", "host2", "host3"]}"#);

        let source = Json::file("config.json");
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
    fn test_json_all_value_types() {
        let env = MockEnv::new().with_file(
            "config.json",
            r#"{
                "string_val": "hello",
                "int_val": 42,
                "float_val": 2.72,
                "bool_val": true,
                "null_val": null
            }"#,
        );

        let source = Json::file("config.json");
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
    fn test_json_parse_error_with_location() {
        let env = MockEnv::new().with_file("config.json", r#"{"host": "localhost", "port": }"#);

        let source = Json::file("config.json");
        let result = source.load(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        match errors.first() {
            ConfigError::SourceError { kind, .. } => {
                assert!(matches!(
                    kind,
                    SourceErrorKind::ParseError {
                        line: Some(_),
                        column: Some(_),
                        ..
                    }
                ));
            }
            _ => panic!("Expected SourceError"),
        }
    }

    #[test]
    fn test_json_custom_name() {
        let env = MockEnv::new().with_file("config.json", r#"{"host": "localhost"}"#);

        let source = Json::file("config.json").named("production config");
        assert_eq!(source.name(), "production config");

        let values = source.load(&env).expect("should load successfully");
        assert_eq!(
            values.get("host").unwrap().source.source,
            "production config"
        );
    }

    #[test]
    fn test_json_required_method() {
        let source = Json::file("config.json").optional().required();
        let env = MockEnv::new();
        let result = source.load(&env);

        // Should fail because file is missing and source is required
        assert!(result.is_err());
    }

    #[test]
    fn test_json_source_location_tracking() {
        let env = MockEnv::new().with_file("config.json", r#"{"host": "localhost"}"#);

        let source = Json::file("config.json");
        let values = source.load(&env).expect("should load successfully");

        let host_value = values.get("host").expect("host should exist");
        assert_eq!(host_value.source.source, "config.json");
    }

    #[test]
    fn test_json_array_of_objects() {
        let env = MockEnv::new().with_file(
            "config.json",
            r#"{
                "servers": [
                    {"name": "server1", "port": 8080},
                    {"name": "server2", "port": 8081}
                ]
            }"#,
        );

        let source = Json::file("config.json");
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
    fn test_json_large_integer() {
        let env = MockEnv::new().with_file("config.json", r#"{"large_int": 9007199254740992}"#);

        let source = Json::file("config.json");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("large_int").map(|v| v.value.as_integer()),
            Some(Some(9007199254740992))
        );
    }

    #[test]
    fn test_json_negative_numbers() {
        let env = MockEnv::new().with_file(
            "config.json",
            r#"{"negative_int": -42, "negative_float": -3.14}"#,
        );

        let source = Json::file("config.json");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("negative_int").map(|v| v.value.as_integer()),
            Some(Some(-42))
        );
        assert_eq!(
            values.get("negative_float").map(|v| v.value.as_float()),
            Some(Some(-3.14))
        );
    }
}
