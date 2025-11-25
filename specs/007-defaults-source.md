---
number: 7
title: Defaults Source
category: storage
priority: medium
status: draft
dependencies: [1, 2]
created: 2025-11-25
---

# Specification 007: Defaults Source

**Category**: storage
**Priority**: medium
**Status**: draft
**Dependencies**: [001 - Core Config Builder, 002 - Error Types]

## Context

Applications need sensible default values for configuration. The defaults source provides a way to specify fallback values that are used when no other source provides a value. This is typically the lowest-priority source in the chain.

## Objective

Implement a `Defaults` source that provides default configuration values from a struct, closure, or explicit path-value pairs.

## Requirements

### Functional Requirements

1. **From Struct**: Load defaults from a struct implementing `Default`
2. **From Closure**: Load defaults from a closure returning the config type
3. **Partial Defaults**: Set defaults for specific paths only
4. **Serialization**: Convert struct to intermediate representation

### Non-Functional Requirements

- Must work with any `Serialize` type
- Clear source attribution as "defaults"
- Efficient (no file I/O)

## Acceptance Criteria

- [ ] `Defaults::from(T::default())` loads from Default impl
- [ ] `Defaults::from_fn(|| ...)` loads from closure
- [ ] `Defaults::partial().set("path", value)` sets specific defaults
- [ ] Defaults are lowest priority (overridden by other sources)
- [ ] Source location shows "defaults" or "defaults:path"
- [ ] Works with nested structs
- [ ] Unit tests for all construction methods

## Technical Details

### API Design

```rust
/// Default values configuration source
pub struct Defaults<T> {
    source: DefaultsSource<T>,
}

enum DefaultsSource<T> {
    Value(T),
    Fn(Box<dyn Fn() -> T + Send + Sync>),
    Partial(PartialDefaults),
}

/// Partial defaults for specific paths
pub struct PartialDefaults {
    values: BTreeMap<String, Value>,
}

impl<T: Serialize> Defaults<T> {
    /// Create defaults from a value
    ///
    /// # Example
    /// ```
    /// Defaults::from(AppConfig::default())
    /// ```
    pub fn from(value: T) -> Self {
        Self {
            source: DefaultsSource::Value(value),
        }
    }
}

impl<T: Serialize> Defaults<T> {
    /// Create defaults from a closure
    ///
    /// # Example
    /// ```
    /// Defaults::from_fn(|| AppConfig {
    ///     server: ServerConfig {
    ///         port: 8080,
    ///         ..Default::default()
    ///     },
    ///     ..Default::default()
    /// })
    /// ```
    pub fn from_fn<F>(f: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            source: DefaultsSource::Fn(Box::new(f)),
        }
    }
}

impl PartialDefaults {
    /// Create empty partial defaults builder
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Set a default value for a specific path
    ///
    /// # Example
    /// ```
    /// Defaults::partial()
    ///     .set("server.port", 8080)
    ///     .set("database.pool_size", 10)
    ///     .set("features.debug", false)
    /// ```
    pub fn set<V: Into<Value>>(mut self, path: impl Into<String>, value: V) -> Self {
        self.values.insert(path.into(), value.into());
        self
    }

    /// Set multiple defaults from an iterator
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

impl Defaults<()> {
    /// Create partial defaults builder
    ///
    /// # Example
    /// ```
    /// let source = Defaults::partial()
    ///     .set("server.timeout_seconds", 30)
    ///     .set("database.pool_size", 10);
    /// ```
    pub fn partial() -> PartialDefaults {
        PartialDefaults::new()
    }
}
```

### Source Implementation

```rust
impl<T: Serialize + Send + Sync> Source for Defaults<T> {
    fn load(&self) -> Validation<ConfigValues, Vec<ConfigError>> {
        match &self.source {
            DefaultsSource::Value(value) => serialize_to_config_values(value, "defaults"),
            DefaultsSource::Fn(f) => {
                let value = f();
                serialize_to_config_values(&value, "defaults")
            }
            DefaultsSource::Partial(partial) => {
                // Partial defaults handled separately
                unreachable!()
            }
        }
    }

    fn name(&self) -> &str {
        "defaults"
    }
}

impl Source for PartialDefaults {
    fn load(&self) -> Validation<ConfigValues, Vec<ConfigError>> {
        let mut config_values = ConfigValues::new();

        for (path, value) in &self.values {
            config_values.insert(
                path.clone(),
                ConfigValue {
                    value: value.clone(),
                    source: SourceLocation::new(format!("defaults:{}", path)),
                },
            );
        }

        Validation::success(config_values)
    }

    fn name(&self) -> &str {
        "defaults"
    }
}
```

### Serialization to ConfigValues

```rust
fn serialize_to_config_values<T: Serialize>(
    value: &T,
    source_name: &str,
) -> Validation<ConfigValues, Vec<ConfigError>> {
    // Use serde_json as intermediate format
    let json = match serde_json::to_value(value) {
        Ok(v) => v,
        Err(e) => {
            return Validation::fail(vec![ConfigError::SourceError {
                source_name: source_name.to_string(),
                kind: SourceErrorKind::Other {
                    message: format!("Failed to serialize defaults: {}", e),
                },
            }]);
        }
    };

    let mut values = ConfigValues::new();
    flatten_json(&json, "", source_name, &mut values);
    Validation::success(values)
}

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
        }
        _ => {
            values.insert(
                prefix.to_string(),
                ConfigValue {
                    value: json_to_value(value),
                    source: SourceLocation::new(source_name),
                },
            );
        }
    }
}

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
                Value::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => {
            Value::Array(arr.iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(map) => {
            Value::Table(map.iter().map(|(k, v)| (k.clone(), json_to_value(v))).collect())
        }
    }
}
```

### Usage Example

```rust
// Full defaults from Default trait
let config = Config::<AppConfig>::builder()
    .source(Defaults::from(AppConfig::default()))
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))
    .build();

// Defaults from closure (for computed defaults)
let config = Config::<AppConfig>::builder()
    .source(Defaults::from_fn(|| AppConfig {
        server: ServerConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], 8080)),
            timeout_seconds: if cfg!(debug_assertions) { 300 } else { 30 },
            ..Default::default()
        },
        ..Default::default()
    }))
    .source(Toml::file("config.toml"))
    .build();

// Partial defaults (only some fields)
let config = Config::<AppConfig>::builder()
    .source(Defaults::partial()
        .set("server.timeout_seconds", 30)
        .set("database.pool_size", 10)
        .set("cache.enabled", false))
    .source(Toml::file("config.toml"))
    .build();
```

## Dependencies

- **Prerequisites**: Specs 001, 002
- **Affected Components**: Config builder integration
- **External Dependencies**:
  - `serde` for serialization
  - `serde_json` as intermediate format

## Testing Strategy

- **Unit Tests**:
  - Defaults from struct
  - Defaults from closure
  - Partial defaults
  - Nested struct serialization
  - Array handling
- **Integration Tests**:
  - Combined with file/env sources
  - Override behavior verification

## Documentation Requirements

- **Code Documentation**: Doc comments with usage examples
- **User Documentation**: Defaults configuration guide

## Implementation Notes

- serde_json used as intermediate because it's already a common dependency
- Consider direct serialization without JSON for better performance later
- Closure defaults are re-evaluated on each build (intentional for dynamic defaults)
- Partial defaults don't require the full config type

## Migration and Compatibility

Not applicable - new project.
