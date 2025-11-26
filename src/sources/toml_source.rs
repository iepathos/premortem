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

use toml_edit::{ImDocument, Item};

use crate::env::ConfigEnv;
use crate::error::{ConfigError, ConfigErrors, SourceErrorKind, SourceLocation};
use crate::source::{ConfigValues, Source};
use crate::sources::line_from_offset;
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
/// Uses toml_edit's ImDocument to capture line numbers for each value.
fn parse_toml(content: &str, source_name: &str) -> Result<ConfigValues, ConfigErrors> {
    // Use ImDocument to preserve span information (DocumentMut loses span info)
    let document: ImDocument<&str> =
        ImDocument::parse(content).map_err(|e: toml_edit::TomlError| {
            // Extract line/column from the parse error
            let span = e.span();
            let (line, column) = span
                .map(|s| {
                    let line = line_from_offset(content, s.start);
                    let last_newline = content[..s.start.min(content.len())]
                        .rfind('\n')
                        .map(|p| p + 1)
                        .unwrap_or(0);
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

    // Pure transformation: TOML -> ConfigValues with line tracking
    let mut values = ConfigValues::empty();
    flatten_document(document.as_item(), "", source_name, content, &mut values);
    Ok(values)
}

/// Pure function: recursively flatten toml_edit document to dot-notation paths.
/// Uses span information from toml_edit to track line numbers.
fn flatten_document(
    item: &Item,
    prefix: &str,
    source_name: &str,
    content: &str,
    values: &mut ConfigValues,
) {
    match item {
        Item::Table(table) => {
            for (key, val) in table.iter() {
                let path = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_document(val, &path, source_name, content, values);
            }
        }
        Item::ArrayOfTables(arr) => {
            for (i, table) in arr.iter().enumerate() {
                let path = format!("{}[{}]", prefix, i);
                // Wrap table in Item to recurse
                for (key, val) in table.iter() {
                    let nested_path = format!("{}.{}", path, key);
                    flatten_document(val, &nested_path, source_name, content, values);
                }
            }
            // Store array length
            values.insert(
                format!("{}.__len", prefix),
                ConfigValue::new(
                    Value::Integer(arr.len() as i64),
                    SourceLocation::new(source_name),
                ),
            );
        }
        Item::Value(v) => {
            // Extract line number from span
            let line = v.span().map(|s| line_from_offset(content, s.start));
            let mut loc = SourceLocation::new(source_name);
            if let Some(l) = line {
                loc = loc.with_line(l);
            }

            // Handle arrays within values
            if let Some(arr) = v.as_array() {
                for (i, item) in arr.iter().enumerate() {
                    let item_path = format!("{}[{}]", prefix, i);
                    let item_line = item.span().map(|s| line_from_offset(content, s.start));
                    let mut item_loc = SourceLocation::new(source_name);
                    if let Some(l) = item_line {
                        item_loc = item_loc.with_line(l);
                    }
                    values.insert(
                        item_path,
                        ConfigValue::new(toml_edit_value_to_value(item), item_loc),
                    );
                }
                // Store array length
                values.insert(
                    format!("{}.__len", prefix),
                    ConfigValue::new(Value::Integer(arr.len() as i64), loc),
                );
            } else if let Some(table) = v.as_inline_table() {
                // Handle inline tables
                for (key, val) in table.iter() {
                    let nested_path = format!("{}.{}", prefix, key);
                    let item_line = val.span().map(|s| line_from_offset(content, s.start));
                    let mut item_loc = SourceLocation::new(source_name);
                    if let Some(l) = item_line {
                        item_loc = item_loc.with_line(l);
                    }
                    values.insert(
                        nested_path,
                        ConfigValue::new(toml_edit_value_to_value(val), item_loc),
                    );
                }
            } else {
                values.insert(
                    prefix.to_string(),
                    ConfigValue::new(toml_edit_value_to_value(v), loc),
                );
            }
        }
        Item::None => {}
    }
}

/// Convert a toml_edit::Value to our Value type.
fn toml_edit_value_to_value(v: &toml_edit::Value) -> Value {
    match v {
        toml_edit::Value::String(s) => Value::String(s.value().to_string()),
        toml_edit::Value::Integer(i) => Value::Integer(*i.value()),
        toml_edit::Value::Float(f) => Value::Float(*f.value()),
        toml_edit::Value::Boolean(b) => Value::Bool(*b.value()),
        toml_edit::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml_edit::Value::Array(arr) => {
            Value::Array(arr.iter().map(toml_edit_value_to_value).collect())
        }
        toml_edit::Value::InlineTable(t) => Value::Table(
            t.iter()
                .map(|(k, v)| (k.to_string(), toml_edit_value_to_value(v)))
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

    #[test]
    fn test_toml_line_number_tracking() {
        let env = MockEnv::new().with_file(
            "config.toml",
            "host = \"localhost\"\nport = 8080\ndebug = true",
        );

        let source = Toml::file("config.toml");
        let values = source.load(&env).expect("should load successfully");

        // host is on line 1
        let host_value = values.get("host").expect("host should exist");
        assert_eq!(host_value.source.source, "config.toml");
        assert_eq!(host_value.source.line, Some(1));

        // port is on line 2
        let port_value = values.get("port").expect("port should exist");
        assert_eq!(port_value.source.line, Some(2));

        // debug is on line 3
        let debug_value = values.get("debug").expect("debug should exist");
        assert_eq!(debug_value.source.line, Some(3));
    }

    #[test]
    fn test_toml_nested_table_line_tracking() {
        let env = MockEnv::new().with_file(
            "config.toml",
            "[database]\nhost = \"localhost\"\nport = 5432",
        );

        let source = Toml::file("config.toml");
        let values = source.load(&env).expect("should load successfully");

        // database.host is on line 2
        let host_value = values.get("database.host").expect("host should exist");
        assert_eq!(host_value.source.line, Some(2));

        // database.port is on line 3
        let port_value = values.get("database.port").expect("port should exist");
        assert_eq!(port_value.source.line, Some(3));
    }
}
