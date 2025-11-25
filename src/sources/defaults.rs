//! Defaults configuration source.
//!
//! This module provides the `Defaults` source for providing default configuration values.
//! Defaults are typically the lowest-priority source, overridden by files and environment.
//!
//! # Example
//!
//! ```ignore
//! use premortem::{Config, Defaults};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Clone, Default, Serialize, Deserialize)]
//! struct AppConfig {
//!     host: String,
//!     port: u16,
//! }
//!
//! // Load from Default trait
//! let config = Config::<AppConfig>::builder()
//!     .source(Defaults::from(AppConfig::default()))
//!     .source(Toml::file("config.toml"))
//!     .build()?;
//!
//! // Load from closure
//! let config = Config::<AppConfig>::builder()
//!     .source(Defaults::from_fn(|| AppConfig {
//!         host: "localhost".to_string(),
//!         port: 8080,
//!     }))
//!     .source(Toml::file("config.toml"))
//!     .build()?;
//!
//! // Partial defaults (specific paths only)
//! let config = Config::<AppConfig>::builder()
//!     .source(Defaults::partial()
//!         .set("host", "localhost")
//!         .set("port", 8080))
//!     .source(Toml::file("config.toml"))
//!     .build()?;
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use crate::env::ConfigEnv;
use crate::error::{ConfigError, ConfigErrors, SourceErrorKind, SourceLocation};
use crate::source::{ConfigValues, Source};
use crate::value::{ConfigValue, Value};

/// The internal source type for defaults.
enum DefaultsSource<T> {
    /// A concrete value
    Value(T),
    /// A closure that produces the value
    Fn(Arc<dyn Fn() -> T + Send + Sync>),
}

impl<T: Clone> Clone for DefaultsSource<T> {
    fn clone(&self) -> Self {
        match self {
            DefaultsSource::Value(v) => DefaultsSource::Value(v.clone()),
            DefaultsSource::Fn(f) => DefaultsSource::Fn(Arc::clone(f)),
        }
    }
}

/// Default values configuration source.
///
/// Provides default values that are used when no other source provides a value.
/// This is typically the lowest-priority source in the chain.
///
/// # Example
///
/// ```ignore
/// use premortem::Defaults;
///
/// // From a value implementing Default
/// let source = Defaults::from(AppConfig::default());
///
/// // From a closure
/// let source = Defaults::from_fn(|| AppConfig {
///     host: "localhost".to_string(),
///     port: 8080,
/// });
///
/// // Partial defaults for specific paths
/// let source = Defaults::partial()
///     .set("server.port", 8080)
///     .set("database.pool_size", 10);
/// ```
pub struct Defaults<T> {
    source: DefaultsSource<T>,
}

impl<T: Serialize + Clone + Send + Sync + 'static> Defaults<T> {
    /// Create defaults from a value.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Defaults;
    ///
    /// let source = Defaults::from(AppConfig::default());
    /// ```
    pub fn from(value: T) -> Self {
        Self {
            source: DefaultsSource::Value(value),
        }
    }

    /// Create defaults from a closure.
    ///
    /// The closure is called each time the defaults are loaded.
    /// This is useful for computed defaults that depend on runtime conditions.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Defaults;
    ///
    /// let source = Defaults::from_fn(|| AppConfig {
    ///     host: "localhost".to_string(),
    ///     port: if cfg!(debug_assertions) { 3000 } else { 8080 },
    /// });
    /// ```
    pub fn from_fn<F>(f: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            source: DefaultsSource::Fn(Arc::new(f)),
        }
    }
}

impl<T: Clone> Clone for Defaults<T> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
        }
    }
}

impl Defaults<()> {
    /// Create a partial defaults builder.
    ///
    /// Use this when you only want to provide defaults for specific configuration paths
    /// rather than the entire configuration structure.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Defaults;
    ///
    /// let source = Defaults::partial()
    ///     .set("server.timeout_seconds", 30)
    ///     .set("database.pool_size", 10)
    ///     .set("cache.enabled", false);
    /// ```
    pub fn partial() -> PartialDefaults {
        PartialDefaults::new()
    }
}

#[cfg(feature = "watch")]
use std::path::PathBuf;

impl<T: Serialize + Clone + Send + Sync + 'static> Source for Defaults<T> {
    /// Load defaults.
    ///
    /// # Note on ConfigEnv
    ///
    /// Defaults have no I/O, so ConfigEnv is unused. The trait parameter
    /// is required for API consistency with other sources.
    fn load(&self, _env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
        let value = match &self.source {
            DefaultsSource::Value(v) => v.clone(),
            DefaultsSource::Fn(f) => f(),
        };

        serialize_to_config_values(&value, "defaults")
    }

    fn name(&self) -> &str {
        "defaults"
    }

    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        // Defaults are not watched - they're static values
        None
    }

    #[cfg(feature = "watch")]
    fn clone_box(&self) -> Box<dyn Source> {
        Box::new(self.clone())
    }
}

/// Partial defaults for specific configuration paths.
///
/// This allows setting defaults for specific paths without needing
/// to define the entire configuration structure.
///
/// # Example
///
/// ```ignore
/// use premortem::Defaults;
///
/// let source = Defaults::partial()
///     .set("server.port", 8080)
///     .set("server.host", "localhost")
///     .set("database.pool_size", 10);
/// ```
#[derive(Debug, Clone, Default)]
pub struct PartialDefaults {
    values: BTreeMap<String, Value>,
}

impl PartialDefaults {
    /// Create empty partial defaults.
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Set a default value for a specific path.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Defaults;
    ///
    /// let source = Defaults::partial()
    ///     .set("server.port", 8080)
    ///     .set("server.host", "localhost");
    /// ```
    pub fn set<V: Into<Value>>(mut self, path: impl Into<String>, value: V) -> Self {
        self.values.insert(path.into(), value.into());
        self
    }

    /// Set multiple defaults from an iterator.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Defaults;
    ///
    /// let defaults = vec![
    ///     ("server.port", 8080.into()),
    ///     ("database.pool_size", 10.into()),
    /// ];
    ///
    /// let source = Defaults::partial().set_many(defaults);
    /// ```
    pub fn set_many<I, K, V>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<Value>,
    {
        for (path, value) in iter {
            self.values.insert(path.into(), value.into());
        }
        self
    }
}

impl Source for PartialDefaults {
    /// Load partial defaults.
    ///
    /// No I/O needed - ConfigEnv is unused but required for trait consistency.
    fn load(&self, _env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
        let mut config_values = ConfigValues::empty();

        for (path, value) in &self.values {
            config_values.insert(
                path.clone(),
                ConfigValue::new(
                    value.clone(),
                    SourceLocation::new(format!("defaults:{}", path)),
                ),
            );
        }

        Ok(config_values)
    }

    fn name(&self) -> &str {
        "defaults"
    }

    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        // Partial defaults are not watched - they're static values
        None
    }

    #[cfg(feature = "watch")]
    fn clone_box(&self) -> Box<dyn Source> {
        Box::new(self.clone())
    }
}

/// Pure function: serialize a value to ConfigValues.
fn serialize_to_config_values<T: Serialize>(
    value: &T,
    source_name: &str,
) -> Result<ConfigValues, ConfigErrors> {
    // Use serde_json as intermediate format
    let json = serde_json::to_value(value).map_err(|e| {
        ConfigErrors::single(ConfigError::SourceError {
            source_name: source_name.to_string(),
            kind: SourceErrorKind::Other {
                message: format!("Failed to serialize defaults: {}", e),
            },
        })
    })?;

    let mut values = ConfigValues::empty();
    flatten_json(&json, "", source_name, &mut values);
    Ok(values)
}

/// Recursively flatten JSON structure to dot-notation paths.
fn flatten_json(
    value: &serde_json::Value,
    prefix: &str,
    source_name: &str,
    values: &mut ConfigValues,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_json(val, &path, source_name, values);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let path = format!("{}[{}]", prefix, i);
                flatten_json(val, &path, source_name, values);
            }
            // Store array length for validation (matching TOML source behavior)
            values.insert(
                format!("{}.__len", prefix),
                ConfigValue::new(
                    Value::Integer(arr.len() as i64),
                    SourceLocation::new(source_name),
                ),
            );
        }
        _ => {
            values.insert(
                prefix.to_string(),
                ConfigValue::new(json_to_value(value), SourceLocation::new(source_name)),
            );
        }
    }
}

/// Convert serde_json::Value to our Value type.
fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                // Fallback for large numbers
                Value::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::Array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(map) => Value::Table(
            map.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockEnv;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
    struct SimpleConfig {
        host: String,
        port: u16,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
    struct NestedConfig {
        server: ServerConfig,
        database: DatabaseConfig,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
    struct ServerConfig {
        host: String,
        port: u16,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
    struct DatabaseConfig {
        host: String,
        pool_size: u32,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    struct ConfigWithArrays {
        hosts: Vec<String>,
        ports: Vec<u16>,
    }

    #[test]
    fn test_defaults_from_value() {
        let env = MockEnv::new();
        let config = SimpleConfig {
            host: "localhost".to_string(),
            port: 8080,
        };

        let source = Defaults::from(config);
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
    fn test_defaults_from_default_trait() {
        let env = MockEnv::new();
        let source = Defaults::from(SimpleConfig::default());
        let values = source.load(&env).expect("should load successfully");

        // Default values should be empty string and 0
        assert_eq!(values.get("host").map(|v| v.value.as_str()), Some(Some("")));
        assert_eq!(
            values.get("port").map(|v| v.value.as_integer()),
            Some(Some(0))
        );
    }

    #[test]
    fn test_defaults_from_closure() {
        let env = MockEnv::new();
        let source = Defaults::from_fn(|| SimpleConfig {
            host: "computed".to_string(),
            port: 3000,
        });

        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("host").map(|v| v.value.as_str()),
            Some(Some("computed"))
        );
        assert_eq!(
            values.get("port").map(|v| v.value.as_integer()),
            Some(Some(3000))
        );
    }

    #[test]
    fn test_defaults_closure_called_each_time() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let env = MockEnv::new();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        let source = Defaults::from_fn(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            SimpleConfig {
                host: "localhost".to_string(),
                port: 8080,
            }
        });

        source.load(&env).expect("should load");
        source.load(&env).expect("should load");
        source.load(&env).expect("should load");

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_defaults_nested_structs() {
        let env = MockEnv::new();
        let config = NestedConfig {
            server: ServerConfig {
                host: "localhost".to_string(),
                port: 8080,
            },
            database: DatabaseConfig {
                host: "db.example.com".to_string(),
                pool_size: 10,
            },
        };

        let source = Defaults::from(config);
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("server.host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert_eq!(
            values.get("server.port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
        assert_eq!(
            values.get("database.host").map(|v| v.value.as_str()),
            Some(Some("db.example.com"))
        );
        assert_eq!(
            values
                .get("database.pool_size")
                .map(|v| v.value.as_integer()),
            Some(Some(10))
        );
    }

    #[test]
    fn test_defaults_with_arrays() {
        let env = MockEnv::new();
        let config = ConfigWithArrays {
            hosts: vec!["host1".to_string(), "host2".to_string()],
            ports: vec![8080, 8081, 8082],
        };

        let source = Defaults::from(config);
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
            values.get("hosts.__len").map(|v| v.value.as_integer()),
            Some(Some(2))
        );
        assert_eq!(
            values.get("ports[0]").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
        assert_eq!(
            values.get("ports[1]").map(|v| v.value.as_integer()),
            Some(Some(8081))
        );
        assert_eq!(
            values.get("ports[2]").map(|v| v.value.as_integer()),
            Some(Some(8082))
        );
        assert_eq!(
            values.get("ports.__len").map(|v| v.value.as_integer()),
            Some(Some(3))
        );
    }

    #[test]
    fn test_partial_defaults_basic() {
        let env = MockEnv::new();
        let source = Defaults::partial()
            .set("server.port", 8080i64)
            .set("database.pool_size", 10i64);

        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("server.port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
        assert_eq!(
            values
                .get("database.pool_size")
                .map(|v| v.value.as_integer()),
            Some(Some(10))
        );
    }

    #[test]
    fn test_partial_defaults_various_types() {
        let env = MockEnv::new();
        let source = Defaults::partial()
            .set("string_val", "hello")
            .set("int_val", 42i64)
            .set("float_val", 2.72f64)
            .set("bool_val", true);

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
    }

    #[test]
    fn test_partial_defaults_set_many() {
        let env = MockEnv::new();
        let defaults = vec![
            ("server.port", Value::Integer(8080)),
            ("server.host", Value::String("localhost".to_string())),
            ("debug", Value::Bool(true)),
        ];

        let source = Defaults::partial().set_many(defaults);
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("server.port").map(|v| v.value.as_integer()),
            Some(Some(8080))
        );
        assert_eq!(
            values.get("server.host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
        assert_eq!(
            values.get("debug").map(|v| v.value.as_bool()),
            Some(Some(true))
        );
    }

    #[test]
    fn test_partial_defaults_source_location() {
        let env = MockEnv::new();
        let source = Defaults::partial().set("server.port", 8080i64);

        let values = source.load(&env).expect("should load successfully");

        let port_value = values.get("server.port").expect("should exist");
        assert_eq!(port_value.source.source, "defaults:server.port");
    }

    #[test]
    fn test_defaults_source_location() {
        let env = MockEnv::new();
        let source = Defaults::from(SimpleConfig {
            host: "localhost".to_string(),
            port: 8080,
        });

        let values = source.load(&env).expect("should load successfully");

        let host_value = values.get("host").expect("should exist");
        assert_eq!(host_value.source.source, "defaults");
    }

    #[test]
    fn test_defaults_name() {
        let source = Defaults::from(SimpleConfig::default());
        assert_eq!(source.name(), "defaults");

        let partial = Defaults::partial().set("key", "value");
        assert_eq!(partial.name(), "defaults");
    }

    #[test]
    fn test_defaults_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Defaults<SimpleConfig>>();
        assert_send_sync::<PartialDefaults>();
    }

    #[test]
    fn test_defaults_clone() {
        let source = Defaults::from(SimpleConfig {
            host: "localhost".to_string(),
            port: 8080,
        });
        let cloned = source.clone();

        let env = MockEnv::new();
        let values1 = source.load(&env).expect("should load");
        let values2 = cloned.load(&env).expect("should load");

        assert_eq!(
            values1.get("host").map(|v| v.value.as_str()),
            values2.get("host").map(|v| v.value.as_str())
        );
    }

    #[test]
    fn test_partial_defaults_empty() {
        let env = MockEnv::new();
        let source = PartialDefaults::new();
        let values = source.load(&env).expect("should load successfully");

        assert!(values.is_empty());
    }

    #[test]
    fn test_json_to_value_null() {
        let json = serde_json::Value::Null;
        let value = json_to_value(&json);
        assert!(value.is_null());
    }

    #[test]
    fn test_json_to_value_bool() {
        let json = serde_json::Value::Bool(true);
        let value = json_to_value(&json);
        assert_eq!(value.as_bool(), Some(true));
    }

    #[test]
    fn test_json_to_value_integer() {
        let json = serde_json::json!(42);
        let value = json_to_value(&json);
        assert_eq!(value.as_integer(), Some(42));
    }

    #[test]
    fn test_json_to_value_float() {
        let json = serde_json::json!(1.5);
        let value = json_to_value(&json);
        assert_eq!(value.as_float(), Some(1.5));
    }

    #[test]
    fn test_json_to_value_string() {
        let json = serde_json::json!("hello");
        let value = json_to_value(&json);
        assert_eq!(value.as_str(), Some("hello"));
    }

    #[test]
    fn test_json_to_value_array() {
        let json = serde_json::json!([1, 2, 3]);
        let value = json_to_value(&json);
        let arr = value.as_array().expect("should be array");
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn test_json_to_value_object() {
        let json = serde_json::json!({"key": "value"});
        let value = json_to_value(&json);
        let table = value.as_table().expect("should be table");
        assert_eq!(table.get("key").and_then(|v| v.as_str()), Some("value"));
    }
}
