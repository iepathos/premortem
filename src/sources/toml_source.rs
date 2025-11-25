//! TOML configuration source.
//!
//! This module provides the `Toml` source for loading configuration from TOML files
//! or strings. It supports required/optional files, string content, and rich error
//! reporting with source locations.
//!
//! # Example
//!
//! ```ignore
//! use premortem::{Config, Toml};
//!
//! // Load from file (required by default)
//! let config = Config::<AppConfig>::builder()
//!     .source(Toml::file("config.toml"))
//!     .build()?;
//!
//! // Load from optional file
//! let config = Config::<AppConfig>::builder()
//!     .source(Toml::file("config.toml").optional())
//!     .build()?;
//!
//! // Load from string
//! let config = Config::<AppConfig>::builder()
//!     .source(Toml::string(r#"
//!         host = "localhost"
//!         port = 8080
//!     "#))
//!     .build()?;
//! ```

use std::path::PathBuf;

use crate::env::ConfigEnv;
use crate::error::{ConfigError, ConfigErrors, SourceErrorKind, SourceLocation};
use crate::source::{ConfigValues, Source};
use crate::value::{ConfigValue, Value};

/// The source type for TOML configuration.
#[derive(Debug, Clone)]
enum TomlSource {
    /// Load from a file path
    File(PathBuf),
    /// Load from a string
    String { content: String, name: String },
}

/// TOML configuration source.
///
/// Loads configuration from TOML files or strings with support for
/// required/optional files and rich error reporting.
#[derive(Debug, Clone)]
pub struct Toml {
    source: TomlSource,
    required: bool,
    name: Option<String>,
}

impl Toml {
    /// Load TOML from a file path (required by default).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Toml;
    ///
    /// let source = Toml::file("config.toml");
    /// let source = Toml::file("/etc/myapp/config.toml");
    /// ```
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            source: TomlSource::File(path.into()),
            required: true,
            name: None,
        }
    }

    /// Load TOML from a string.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Toml;
    ///
    /// let source = Toml::string(r#"
    ///     host = "localhost"
    ///     port = 8080
    /// "#);
    /// ```
    pub fn string(content: impl Into<String>) -> Self {
        Self {
            source: TomlSource::String {
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
    /// use premortem::Toml;
    ///
    /// let source = Toml::file("config.toml").optional();
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
    /// use premortem::Toml;
    ///
    /// let source = Toml::file("config.toml").required();
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
    /// use premortem::Toml;
    ///
    /// let source = Toml::file("config.toml").named("base configuration");
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
            TomlSource::File(path) => path.display().to_string(),
            TomlSource::String { name, .. } => name.clone(),
        }
    }
}

impl Source for Toml {
    /// Load TOML configuration.
    ///
    /// File I/O is performed through the `ConfigEnv` trait, enabling
    /// dependency injection for testing. Parsing is pure and happens
    /// after the I/O completes.
    fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
        let source_name = self.source_name();

        let content = match &self.source {
            TomlSource::File(path) => match env.read_file(path) {
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
            TomlSource::String { content, .. } => content.clone(),
        };

        // Pure parsing (after I/O)
        parse_toml(&content, &source_name)
    }

    fn name(&self) -> &str {
        match &self.name {
            Some(name) => name,
            None => match &self.source {
                TomlSource::File(path) => path.to_str().unwrap_or("<file>"),
                TomlSource::String { name, .. } => name,
            },
        }
    }

    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        match &self.source {
            TomlSource::File(path) => Some(path.clone()),
            TomlSource::String { .. } => None,
        }
    }

    #[cfg(feature = "watch")]
    fn clone_box(&self) -> Box<dyn Source> {
        Box::new(self.clone())
    }
}

/// Pure function: parse TOML content into ConfigValues.
/// No I/O - this runs after the Effect has read the file.
fn parse_toml(content: &str, source_name: &str) -> Result<ConfigValues, ConfigErrors> {
    let document: toml::Value = content.parse().map_err(|e: toml::de::Error| {
        // Extract line/column from the TOML error
        let span = e.span();
        let (line, column) = span
            .map(|s| {
                // Calculate line and column from byte offset
                let before = &content[..s.start.min(content.len())];
                let line = before.lines().count() as u32;
                let last_newline = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
                let column = (s.start - last_newline + 1) as u32;
                (Some(line), Some(column))
            })
            .unwrap_or((None, None));

        ConfigErrors::single(ConfigError::SourceError {
            source_name: source_name.to_string(),
            kind: SourceErrorKind::ParseError {
                message: e.message().to_string(),
                line,
                column,
            },
        })
    })?;

    // Pure transformation: TOML -> ConfigValues
    let mut values = ConfigValues::empty();
    flatten_value(&document, "", source_name, &mut values);
    Ok(values)
}

/// Pure function: recursively flatten TOML structure to dot-notation paths.
fn flatten_value(value: &toml::Value, prefix: &str, source_name: &str, values: &mut ConfigValues) {
    match value {
        toml::Value::Table(table) => {
            for (key, val) in table {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_value(val, &path, source_name, values);
            }
        }
        toml::Value::Array(arr) => {
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
                ConfigValue::new(toml_to_value(value), SourceLocation::new(source_name));
            values.insert(prefix.to_string(), config_value);
        }
    }
}

/// Convert a toml::Value to our Value type.
fn toml_to_value(toml: &toml::Value) -> Value {
    match toml {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(i) => Value::Integer(*i),
        toml::Value::Float(f) => Value::Float(*f),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(arr) => Value::Array(arr.iter().map(toml_to_value).collect()),
        toml::Value::Table(t) => Value::Table(
            t.iter()
                .map(|(k, v)| (k.clone(), toml_to_value(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockEnv;

    #[test]
    fn test_toml_file_load() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            host = "localhost"
            port = 8080
            "#,
        );

        let source = Toml::file("config.toml");
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
    fn test_toml_file_missing_required() {
        let env = MockEnv::new();

        let source = Toml::file("missing.toml");
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
    fn test_toml_file_missing_optional() {
        let env = MockEnv::new();

        let source = Toml::file("missing.toml").optional();
        let values = source.load(&env).expect("should succeed with empty values");

        assert!(values.is_empty());
    }

    #[test]
    fn test_toml_file_permission_denied() {
        let env = MockEnv::new().with_unreadable_file("secret.toml");

        let source = Toml::file("secret.toml");
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
    fn test_toml_string_load() {
        let env = MockEnv::new();

        let source = Toml::string(
            r#"
            host = "localhost"
            port = 8080
            "#,
        );
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
    fn test_toml_nested_tables() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            [database]
            host = "localhost"
            port = 5432

            [database.pool]
            min_size = 5
            max_size = 20
            "#,
        );

        let source = Toml::file("config.toml");
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
    fn test_toml_arrays() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            hosts = ["host1", "host2", "host3"]
            "#,
        );

        let source = Toml::file("config.toml");
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
    fn test_toml_all_value_types() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            string_val = "hello"
            int_val = 42
            float_val = 2.72
            bool_val = true
            date_val = 2024-01-15
            "#,
        );

        let source = Toml::file("config.toml");
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
        // Datetime is converted to string
        assert!(values.get("date_val").map(|v| v.value.as_str()).is_some());
    }

    #[test]
    fn test_toml_parse_error_with_location() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            host = "localhost"
            port = "not a number
            "#,
        );

        let source = Toml::file("config.toml");
        let result = source.load(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        match errors.first() {
            ConfigError::SourceError { kind, .. } => {
                assert!(matches!(
                    kind,
                    SourceErrorKind::ParseError { line: Some(_), .. }
                ));
            }
            _ => panic!("Expected SourceError"),
        }
    }

    #[test]
    fn test_toml_custom_name() {
        let env = MockEnv::new().with_file("config.toml", "host = \"localhost\"");

        let source = Toml::file("config.toml").named("production config");
        assert_eq!(source.name(), "production config");

        let values = source.load(&env).expect("should load successfully");
        assert_eq!(
            values.get("host").unwrap().source.source,
            "production config"
        );
    }

    #[test]
    fn test_toml_required_method() {
        let source = Toml::file("config.toml").optional().required();
        let env = MockEnv::new();
        let result = source.load(&env);

        // Should fail because file is missing and source is required
        assert!(result.is_err());
    }

    #[test]
    fn test_toml_inline_tables() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            server = { host = "localhost", port = 8080 }
            "#,
        );

        let source = Toml::file("config.toml");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("server.host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert_eq!(
            values.get("server.port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
    }

    #[test]
    fn test_toml_array_of_tables() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            [[servers]]
            name = "server1"
            port = 8080

            [[servers]]
            name = "server2"
            port = 8081
            "#,
        );

        let source = Toml::file("config.toml");
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
    fn test_toml_source_location_tracking() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            host = "localhost"
            "#,
        );

        let source = Toml::file("config.toml");
        let values = source.load(&env).expect("should load successfully");

        let host_value = values.get("host").expect("host should exist");
        assert_eq!(host_value.source.source, "config.toml");
    }
}
