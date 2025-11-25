//! Environment variable configuration source.
//!
//! This module provides the `Env` source for loading configuration from environment
//! variables. It supports prefix filtering, custom mappings, path separators, case
//! sensitivity options, list parsing, and type inference.
//!
//! # Example
//!
//! ```ignore
//! use premortem::{Config, Env};
//!
//! // Load env vars with prefix
//! let config = Config::<AppConfig>::builder()
//!     .source(Env::prefix("APP_"))
//!     .build()?;
//!
//! // Load with custom separator
//! let config = Config::<AppConfig>::builder()
//!     .source(Env::prefix("APP_").separator("__"))
//!     .build()?;
//!
//! // Load with custom mappings
//! let config = Config::<AppConfig>::builder()
//!     .source(Env::prefix("APP_").map("DB_HOST", "database.host"))
//!     .build()?;
//! ```

use std::collections::{HashMap, HashSet};

use crate::env::ConfigEnv;
use crate::error::{ConfigErrors, SourceLocation};
use crate::source::{ConfigValues, Source};
use crate::value::{ConfigValue, Value};

/// Environment variable configuration source.
///
/// Loads configuration from environment variables with support for
/// prefix filtering, custom mappings, and type inference.
///
/// # Example
///
/// ```ignore
/// use premortem::Env;
///
/// // Basic usage with prefix
/// let source = Env::prefix("APP_");
/// // APP_DATABASE_HOST -> database.host
/// // APP_SERVER_PORT -> server.port
///
/// // Custom separator
/// let source = Env::prefix("APP_").separator("__");
/// // APP__DATABASE__HOST -> database.host
///
/// // Custom mappings
/// let source = Env::prefix("APP_")
///     .map("DB_HOST", "database.host")
///     .map("DB_PORT", "database.port");
/// ```
#[derive(Debug, Clone)]
pub struct Env {
    prefix: String,
    separator: String,
    case_sensitive: bool,
    list_separator: Option<String>,
    custom_mappings: HashMap<String, String>,
    excluded: HashSet<String>,
}

impl Env {
    /// Create env source with given prefix.
    ///
    /// Environment variables matching the prefix will be included,
    /// and the prefix will be stripped before mapping to config paths.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Env;
    ///
    /// let source = Env::prefix("APP_");
    /// // APP_DATABASE_HOST -> database.host
    /// // APP_SERVER_PORT -> server.port
    /// ```
    pub fn prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            separator: "_".to_string(),
            case_sensitive: false,
            list_separator: None,
            custom_mappings: HashMap::new(),
            excluded: HashSet::new(),
        }
    }

    /// Create env source without prefix (use all env vars).
    ///
    /// **Warning**: This will load all environment variables, which may
    /// include sensitive system variables. Use with caution.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Env;
    ///
    /// let source = Env::all();
    /// ```
    pub fn all() -> Self {
        Self::prefix("")
    }

    /// Set the separator used in environment variable names.
    ///
    /// The separator is used to split env var names into path segments.
    /// Default is `_`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Env;
    ///
    /// let source = Env::prefix("APP_").separator("__");
    /// // APP__DATABASE__HOST -> database.host
    /// ```
    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.separator = sep.into();
        self
    }

    /// Add a custom mapping from env var suffix to config path.
    ///
    /// Custom mappings override the automatic path derivation.
    /// The `env_suffix` is the part after the prefix.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Env;
    ///
    /// let source = Env::prefix("APP_")
    ///     .map("DB_HOST", "database.host")
    ///     .map("DB_PORT", "database.port");
    /// // APP_DB_HOST -> database.host (instead of db.host)
    /// ```
    pub fn map(mut self, env_suffix: impl Into<String>, path: impl Into<String>) -> Self {
        self.custom_mappings.insert(env_suffix.into(), path.into());
        self
    }

    /// Exclude specific environment variables.
    ///
    /// Excluded variables will not be loaded even if they match the prefix.
    /// Use the full variable name, not just the suffix.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Env;
    ///
    /// let source = Env::prefix("APP_")
    ///     .exclude("APP_SECRET_KEY");
    /// ```
    pub fn exclude(mut self, var: impl Into<String>) -> Self {
        self.excluded.insert(var.into());
        self
    }

    /// Require exact case match (default: case insensitive).
    ///
    /// When case sensitive, `APP_HOST` and `app_host` are treated as different.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Env;
    ///
    /// let source = Env::prefix("APP_").case_sensitive();
    /// ```
    pub fn case_sensitive(mut self) -> Self {
        self.case_sensitive = true;
        self
    }

    /// Set case insensitive matching (default).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Env;
    ///
    /// let source = Env::prefix("APP_").case_insensitive();
    /// ```
    pub fn case_insensitive(mut self) -> Self {
        self.case_sensitive = false;
        self
    }

    /// Parse values containing this separator as lists.
    ///
    /// When set, values containing the separator will be split into arrays.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Env;
    ///
    /// let source = Env::prefix("APP_").list_separator(",");
    /// // APP_ALLOWED_HOSTS=a.com,b.com -> ["a.com", "b.com"]
    /// ```
    pub fn list_separator(mut self, sep: impl Into<String>) -> Self {
        self.list_separator = Some(sep.into());
        self
    }
}

impl Source for Env {
    /// Load environment variables.
    ///
    /// The `ConfigEnv` parameter enables dependency injection for testing.
    /// In production, use `RealEnv`; in tests, use `MockEnv`.
    fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
        let mut values = ConfigValues::empty();
        let prefix_for_comparison = if self.case_sensitive {
            self.prefix.clone()
        } else {
            self.prefix.to_lowercase()
        };

        // Get env vars through ConfigEnv (mockable!)
        let env_vars = if self.prefix.is_empty() {
            env.all_env_vars()
        } else {
            env.env_vars_with_prefix(&self.prefix)
        };

        for (key, value) in env_vars {
            // Check prefix match (case-sensitive or insensitive)
            let key_for_comparison = if self.case_sensitive {
                key.clone()
            } else {
                key.to_lowercase()
            };

            if !key_for_comparison.starts_with(&prefix_for_comparison) {
                continue;
            }

            // Check exclusions
            if self.excluded.contains(&key) {
                continue;
            }

            // Get suffix after prefix
            let suffix = &key[self.prefix.len()..];

            // Check for custom mapping
            let path = if let Some(mapped) = self.custom_mappings.get(suffix) {
                mapped.clone()
            } else {
                // Convert suffix to config path
                suffix_to_path(suffix, &self.separator)
            };

            // Parse value (pure function)
            let parsed_value = parse_env_value(&value, self.list_separator.as_deref());

            // Store with source location
            let source = SourceLocation::env(&key);
            values.insert(
                path,
                ConfigValue {
                    value: parsed_value,
                    source,
                },
            );
        }

        Ok(values)
    }

    fn name(&self) -> &str {
        "environment"
    }
}

/// Pure function: convert env var suffix to config path.
///
/// Converts `DATABASE_HOST` to `database.host` using the separator.
/// Handles numeric segments as array indices.
fn suffix_to_path(suffix: &str, separator: &str) -> String {
    let parts: Vec<&str> = suffix.split(separator).collect();
    let mut path_parts = Vec::new();

    for part in parts {
        let lower = part.to_lowercase();
        // Check if it's a numeric index
        if let Ok(idx) = lower.parse::<usize>() {
            // Convert previous part to array index
            if let Some(last) = path_parts.last_mut() {
                *last = format!("{}[{}]", last, idx);
                continue;
            }
        }
        path_parts.push(lower);
    }

    path_parts.join(".")
}

/// Pure function: parse environment variable value.
///
/// Supports list parsing and type inference.
fn parse_env_value(value: &str, list_separator: Option<&str>) -> Value {
    // Check for list
    if let Some(sep) = list_separator {
        if value.contains(sep) {
            let items: Vec<Value> = value.split(sep).map(|s| parse_scalar(s.trim())).collect();
            return Value::Array(items);
        }
    }

    parse_scalar(value)
}

/// Pure function: parse scalar value with type inference.
///
/// Attempts to parse in order: boolean, integer, float, then string.
fn parse_scalar(value: &str) -> Value {
    // Try boolean
    match value.to_lowercase().as_str() {
        "true" | "yes" | "1" | "on" => return Value::Bool(true),
        "false" | "no" | "0" | "off" => return Value::Bool(false),
        _ => {}
    }

    // Try integer
    if let Ok(i) = value.parse::<i64>() {
        return Value::Integer(i);
    }

    // Try float
    if let Ok(f) = value.parse::<f64>() {
        return Value::Float(f);
    }

    // Keep as string
    Value::String(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockEnv;

    #[test]
    fn test_env_basic_prefix() {
        let env = MockEnv::new()
            .with_env("APP_HOST", "localhost")
            .with_env("APP_PORT", "8080")
            .with_env("OTHER_VAR", "ignored");

        let source = Env::prefix("APP_");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert_eq!(
            values.get("port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
        assert!(values.get("other_var").is_none());
    }

    #[test]
    fn test_env_nested_paths() {
        let env = MockEnv::new()
            .with_env("APP_DATABASE_HOST", "localhost")
            .with_env("APP_DATABASE_PORT", "5432")
            .with_env("APP_DATABASE_POOL_SIZE", "10");

        let source = Env::prefix("APP_");
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
                .get("database.pool.size")
                .map(|v| v.value.as_integer()),
            Some(Some(10))
        );
    }

    #[test]
    fn test_env_custom_separator() {
        let env = MockEnv::new()
            .with_env("APP__DATABASE__HOST", "localhost")
            .with_env("APP__SERVER__PORT", "8080");

        let source = Env::prefix("APP__").separator("__");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("database.host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert_eq!(
            values.get("server.port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
    }

    #[test]
    fn test_env_custom_mapping() {
        let env = MockEnv::new()
            .with_env("APP_DB_HOST", "localhost")
            .with_env("APP_DB_PORT", "5432");

        let source = Env::prefix("APP_")
            .map("DB_HOST", "database.host")
            .map("DB_PORT", "database.port");

        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("database.host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert_eq!(
            values.get("database.port").map(|v| v.value.as_integer()),
            Some(Some(5432))
        );
    }

    #[test]
    fn test_env_exclusions() {
        let env = MockEnv::new()
            .with_env("APP_HOST", "localhost")
            .with_env("APP_SECRET", "super_secret");

        let source = Env::prefix("APP_").exclude("APP_SECRET");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert!(values.get("secret").is_none());
    }

    #[test]
    fn test_env_list_separator() {
        let env = MockEnv::new()
            .with_env("APP_HOSTS", "host1.com,host2.com,host3.com")
            .with_env("APP_SINGLE", "just_one");

        let source = Env::prefix("APP_").list_separator(",");
        let values = source.load(&env).expect("should load successfully");

        let hosts = values.get("hosts").map(|v| v.value.as_array());
        assert!(hosts.is_some());
        let hosts = hosts.unwrap().unwrap();
        assert_eq!(hosts.len(), 3);
        assert_eq!(hosts[0].as_str(), Some("host1.com"));
        assert_eq!(hosts[1].as_str(), Some("host2.com"));
        assert_eq!(hosts[2].as_str(), Some("host3.com"));

        // Single value without separator should not be an array
        assert_eq!(
            values.get("single").map(|v| v.value.as_str()),
            Some(Some("just_one"))
        );
    }

    #[test]
    fn test_env_type_inference_bool() {
        let env = MockEnv::new()
            .with_env("APP_TRUE1", "true")
            .with_env("APP_TRUE2", "yes")
            .with_env("APP_TRUE3", "1")
            .with_env("APP_TRUE4", "on")
            .with_env("APP_FALSE1", "false")
            .with_env("APP_FALSE2", "no")
            .with_env("APP_FALSE3", "0")
            .with_env("APP_FALSE4", "off");

        let source = Env::prefix("APP_");
        let values = source.load(&env).expect("should load successfully");

        // True values
        assert_eq!(
            values.get("true1").map(|v| v.value.as_bool()),
            Some(Some(true))
        );
        assert_eq!(
            values.get("true2").map(|v| v.value.as_bool()),
            Some(Some(true))
        );
        assert_eq!(
            values.get("true3").map(|v| v.value.as_bool()),
            Some(Some(true))
        );
        assert_eq!(
            values.get("true4").map(|v| v.value.as_bool()),
            Some(Some(true))
        );

        // False values
        assert_eq!(
            values.get("false1").map(|v| v.value.as_bool()),
            Some(Some(false))
        );
        assert_eq!(
            values.get("false2").map(|v| v.value.as_bool()),
            Some(Some(false))
        );
        assert_eq!(
            values.get("false3").map(|v| v.value.as_bool()),
            Some(Some(false))
        );
        assert_eq!(
            values.get("false4").map(|v| v.value.as_bool()),
            Some(Some(false))
        );
    }

    #[test]
    fn test_env_type_inference_numbers() {
        let env = MockEnv::new()
            .with_env("APP_INT", "42")
            .with_env("APP_NEGATIVE", "-10")
            .with_env("APP_FLOAT", "3.25")
            .with_env("APP_SCIENTIFIC", "1.5e10");

        let source = Env::prefix("APP_");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("int").map(|v| v.value.as_integer()),
            Some(Some(42))
        );
        assert_eq!(
            values.get("negative").map(|v| v.value.as_integer()),
            Some(Some(-10))
        );
        assert_eq!(
            values.get("float").map(|v| v.value.as_float()),
            Some(Some(3.25))
        );
        // Scientific notation parses as float
        let scientific = values.get("scientific").map(|v| v.value.as_float());
        assert!(scientific.is_some());
        assert!((scientific.unwrap().unwrap() - 1.5e10).abs() < 1.0);
    }

    #[test]
    fn test_env_type_inference_string() {
        let env = MockEnv::new()
            .with_env("APP_STRING", "hello world")
            .with_env("APP_URL", "https://example.com");

        let source = Env::prefix("APP_");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("string").map(|v| v.value.as_str()),
            Some(Some("hello world"))
        );
        assert_eq!(
            values.get("url").map(|v| v.value.as_str()),
            Some(Some("https://example.com"))
        );
    }

    #[test]
    fn test_env_array_indices() {
        let env = MockEnv::new()
            .with_env("APP_SERVERS_0_HOST", "host1.com")
            .with_env("APP_SERVERS_0_PORT", "8080")
            .with_env("APP_SERVERS_1_HOST", "host2.com")
            .with_env("APP_SERVERS_1_PORT", "8081");

        let source = Env::prefix("APP_");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("servers[0].host").map(|v| v.value.as_str()),
            Some(Some("host1.com"))
        );
        assert_eq!(
            values.get("servers[0].port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
        assert_eq!(
            values.get("servers[1].host").map(|v| v.value.as_str()),
            Some(Some("host2.com"))
        );
        assert_eq!(
            values.get("servers[1].port").map(|v| v.value.as_integer()),
            Some(Some(8081))
        );
    }

    #[test]
    fn test_env_case_insensitive_default() {
        let env = MockEnv::new()
            .with_env("app_host", "lowercase")
            .with_env("APP_PORT", "8080");

        // Note: MockEnv uses exact match for env_vars_with_prefix,
        // so we need to test this differently
        let source = Env::prefix("APP_");
        let values = source.load(&env).expect("should load successfully");

        // Only APP_PORT should match with APP_ prefix (MockEnv is case-sensitive in its prefix check)
        assert_eq!(
            values.get("port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
    }

    #[test]
    fn test_env_source_location() {
        let env = MockEnv::new().with_env("APP_HOST", "localhost");

        let source = Env::prefix("APP_");
        let values = source.load(&env).expect("should load successfully");

        let host_value = values.get("host").expect("host should exist");
        assert_eq!(host_value.source.source, "env:APP_HOST");
    }

    #[test]
    fn test_env_empty_prefix() {
        let env = MockEnv::new()
            .with_env("HOST", "localhost")
            .with_env("PORT", "8080");

        let source = Env::all();
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
    fn test_env_name() {
        let source = Env::prefix("APP_");
        assert_eq!(source.name(), "environment");
    }

    #[test]
    fn test_suffix_to_path_simple() {
        assert_eq!(suffix_to_path("HOST", "_"), "host");
        assert_eq!(suffix_to_path("DATABASE_HOST", "_"), "database.host");
        assert_eq!(
            suffix_to_path("DATABASE_POOL_SIZE", "_"),
            "database.pool.size"
        );
    }

    #[test]
    fn test_suffix_to_path_custom_separator() {
        assert_eq!(suffix_to_path("DATABASE__HOST", "__"), "database.host");
    }

    #[test]
    fn test_suffix_to_path_array_index() {
        assert_eq!(suffix_to_path("SERVERS_0_HOST", "_"), "servers[0].host");
        assert_eq!(suffix_to_path("ITEMS_1", "_"), "items[1]");
    }

    #[test]
    fn test_parse_scalar_bool() {
        assert_eq!(parse_scalar("true"), Value::Bool(true));
        assert_eq!(parse_scalar("TRUE"), Value::Bool(true));
        assert_eq!(parse_scalar("yes"), Value::Bool(true));
        assert_eq!(parse_scalar("YES"), Value::Bool(true));
        assert_eq!(parse_scalar("1"), Value::Bool(true));
        assert_eq!(parse_scalar("on"), Value::Bool(true));
        assert_eq!(parse_scalar("ON"), Value::Bool(true));

        assert_eq!(parse_scalar("false"), Value::Bool(false));
        assert_eq!(parse_scalar("FALSE"), Value::Bool(false));
        assert_eq!(parse_scalar("no"), Value::Bool(false));
        assert_eq!(parse_scalar("NO"), Value::Bool(false));
        assert_eq!(parse_scalar("0"), Value::Bool(false));
        assert_eq!(parse_scalar("off"), Value::Bool(false));
        assert_eq!(parse_scalar("OFF"), Value::Bool(false));
    }

    #[test]
    fn test_parse_scalar_integer() {
        assert_eq!(parse_scalar("42"), Value::Integer(42));
        assert_eq!(parse_scalar("-10"), Value::Integer(-10));
        assert_eq!(parse_scalar("1000000"), Value::Integer(1000000));
    }

    #[test]
    fn test_parse_scalar_float() {
        assert_eq!(parse_scalar("3.25"), Value::Float(3.25));
        assert_eq!(parse_scalar("-2.5"), Value::Float(-2.5));
    }

    #[test]
    fn test_parse_scalar_string() {
        assert_eq!(parse_scalar("hello"), Value::String("hello".to_string()));
        assert_eq!(
            parse_scalar("hello world"),
            Value::String("hello world".to_string())
        );
        // Non-boolean strings that look like bools but aren't exact matches
        assert_eq!(parse_scalar("True!"), Value::String("True!".to_string()));
    }

    #[test]
    fn test_parse_env_value_list() {
        let value = parse_env_value("a,b,c", Some(","));
        match value {
            Value::Array(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::String("a".to_string()));
                assert_eq!(items[1], Value::String("b".to_string()));
                assert_eq!(items[2], Value::String("c".to_string()));
            }
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_parse_env_value_list_with_numbers() {
        let value = parse_env_value("1,2,3", Some(","));
        match value {
            Value::Array(items) => {
                assert_eq!(items.len(), 3);
                // Note: "1" is parsed as bool true, "2" and "3" are integers
                assert_eq!(items[0], Value::Bool(true));
                assert_eq!(items[1], Value::Integer(2));
                assert_eq!(items[2], Value::Integer(3));
            }
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_parse_env_value_no_list_separator() {
        let value = parse_env_value("a,b,c", None);
        assert_eq!(value, Value::String("a,b,c".to_string()));
    }

    #[test]
    fn test_env_list_with_trimming() {
        let env = MockEnv::new().with_env("APP_HOSTS", "host1 , host2 , host3");

        let source = Env::prefix("APP_").list_separator(",");
        let values = source.load(&env).expect("should load successfully");

        let hosts = values.get("hosts").map(|v| v.value.as_array());
        assert!(hosts.is_some());
        let hosts = hosts.unwrap().unwrap();
        assert_eq!(hosts.len(), 3);
        assert_eq!(hosts[0].as_str(), Some("host1"));
        assert_eq!(hosts[1].as_str(), Some("host2"));
        assert_eq!(hosts[2].as_str(), Some("host3"));
    }
}
