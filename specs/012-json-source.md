---
number: 12
title: JSON Configuration Source
category: compatibility
priority: high
status: draft
dependencies: [1, 2, 3]
created: 2025-11-25
---

# Specification 012: JSON Configuration Source

**Category**: compatibility
**Priority**: high
**Status**: draft
**Dependencies**: [001 - Core Config Builder, 002 - Error Types, 003 - Validate Trait]

## Context

JSON is one of the most widely used configuration formats, especially in JavaScript/Node.js ecosystems and web applications. Many existing applications already have JSON configuration files, and users expect config libraries to support JSON out of the box.

Premortem currently supports TOML (default feature) but not JSON. Adding JSON support expands compatibility with existing projects and aligns with user expectations for a configuration library.

The implementation should mirror the TOML source's API design for consistency, using the same builder pattern, optional/required semantics, and error handling approach.

## Objective

Implement a JSON configuration source that loads configuration from JSON files or strings, with the same API patterns as the existing TOML source, feature-gated behind the `json` feature flag.

## Requirements

### Functional Requirements

1. **File Loading**: Load JSON configuration from file paths
2. **String Loading**: Load JSON configuration from string content
3. **Optional Files**: Support optional files that don't error when missing
4. **Required Files**: Support required files (default) that error when missing
5. **Custom Naming**: Allow custom source names for error messages
6. **Error Reporting**: Rich error messages with line/column information where available
7. **Nested Structures**: Support nested objects and arrays
8. **All JSON Types**: Support strings, numbers, booleans, null, arrays, objects
9. **Source Location Tracking**: Track which JSON file each value came from

### Non-Functional Requirements

- Feature-gated behind `json` feature flag
- Zero-cost when feature is disabled (no dependencies pulled)
- API consistent with TOML source
- Same error types and accumulation patterns
- Thread-safe (implements `Send + Sync`)

## Acceptance Criteria

- [ ] `Json::file("config.json")` loads configuration from file
- [ ] `Json::string(content)` loads configuration from string
- [ ] `Json::file(...).optional()` doesn't error on missing file
- [ ] `Json::file(...).required()` errors on missing file (default)
- [ ] `Json::file(...).named("custom")` sets custom source name
- [ ] Parse errors include line/column when available
- [ ] Nested objects flatten to dot notation (e.g., `server.host`)
- [ ] Arrays flatten to indexed notation (e.g., `hosts[0]`)
- [ ] Array length stored as `path.__len` for validation
- [ ] JSON `null` values handled appropriately
- [ ] Source location tracking works correctly
- [ ] Unit tests cover all functionality
- [ ] Integration with `ConfigBuilder` works identically to TOML

## Technical Details

### API Design

```rust
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

#[derive(Debug, Clone)]
enum JsonSource {
    /// Load from a file path
    File(PathBuf),
    /// Load from a string
    String { content: String, name: String },
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
    pub fn file(path: impl Into<PathBuf>) -> Self;

    /// Load JSON from a string.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::Json;
    ///
    /// let source = Json::string(r#"{"host": "localhost", "port": 8080}"#);
    /// ```
    pub fn string(content: impl Into<String>) -> Self;

    /// Mark this source as optional (no error if file missing).
    pub fn optional(mut self) -> Self;

    /// Mark this source as required (default).
    pub fn required(mut self) -> Self;

    /// Set a custom name for this source in error messages.
    pub fn named(mut self, name: impl Into<String>) -> Self;
}

impl Source for Json {
    fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors>;
    fn name(&self) -> &str;

    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf>;
}
```

### Usage Examples

```rust
use premortem::{Config, Json, Env};

// Load from JSON file
let config = Config::<AppConfig>::builder()
    .source(Json::file("config.json"))
    .build()?;

// Layer with environment variables
let config = Config::<AppConfig>::builder()
    .source(Json::file("config.json"))
    .source(Env::new().prefix("APP"))
    .build()?;

// Optional JSON file with defaults
let config = Config::<AppConfig>::builder()
    .source(Defaults::from::<AppConfig>())
    .source(Json::file("config.json").optional())
    .source(Env::new().prefix("APP"))
    .build()?;

// From string (useful for embedded config or testing)
let config = Config::<AppConfig>::builder()
    .source(Json::string(r#"{"host": "localhost", "port": 8080}"#))
    .build()?;
```

### JSON to Value Mapping

| JSON Type | Value Type |
|-----------|------------|
| `string` | `Value::String` |
| `number` (integer) | `Value::Integer` |
| `number` (float) | `Value::Float` |
| `true`/`false` | `Value::Bool` |
| `null` | `Value::Null` |
| `array` | Flattened to `path[0]`, `path[1]`, etc. |
| `object` | Flattened to `path.key` |

### Null Handling

JSON supports `null` values which TOML does not. The implementation should:

1. Add `Value::Null` variant to the Value enum if not present
2. `null` values should be treated as "value present but empty"
3. Optional fields with `null` should deserialize to `None`
4. Required fields with `null` should produce a validation error

```rust
// In value.rs, ensure Null is supported
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Null,  // Add if not present
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
}
```

### Error Handling

Parse errors should include source location when possible:

```rust
fn parse_json(content: &str, source_name: &str) -> Result<ConfigValues, ConfigErrors> {
    let document: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| {
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
    flatten_json_value(&document, "", source_name, &mut values);
    Ok(values)
}
```

### Module Organization

```
src/sources/
├── mod.rs           # Re-export Json
├── toml_source.rs   # Existing TOML source
├── env_source.rs    # Existing env source
├── defaults.rs      # Existing defaults source
└── json_source.rs   # NEW: JSON source
```

### Cargo.toml Changes

```toml
[features]
default = ["toml", "derive"]
toml = ["dep:toml"]
json = ["dep:serde_json"]  # Already in deps, just gate the source

[dependencies]
serde_json = { version = "1.0", optional = true }  # Make optional if not already
```

### Watch Integration

If the `watch` feature is enabled, JSON files should be watchable:

```rust
#[cfg(feature = "watch")]
impl Source for Json {
    fn watch_path(&self) -> Option<PathBuf> {
        match &self.source {
            JsonSource::File(path) => Some(path.clone()),
            JsonSource::String { .. } => None,
        }
    }
}
```

## Dependencies

- **Prerequisites**: Specs 001, 002, 003
- **Affected Components**: `sources` module, `lib.rs` re-exports, `Value` enum
- **External Dependencies**:
  - `serde_json` crate (already likely a dependency via serde)

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockEnv;

    #[test]
    fn test_json_file_load() {
        let env = MockEnv::new().with_file(
            "config.json",
            r#"{"host": "localhost", "port": 8080}"#,
        );

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
        assert!(matches!(
            errors.first(),
            ConfigError::SourceError { kind: SourceErrorKind::NotFound { .. }, .. }
        ));
    }

    #[test]
    fn test_json_file_missing_optional() {
        let env = MockEnv::new();
        let source = Json::file("missing.json").optional();
        let values = source.load(&env).expect("should succeed with empty values");

        assert!(values.is_empty());
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
            values.get("database.pool.max_size").map(|v| v.value.as_integer()),
            Some(Some(20))
        );
    }

    #[test]
    fn test_json_arrays() {
        let env = MockEnv::new().with_file(
            "config.json",
            r#"{"hosts": ["host1", "host2", "host3"]}"#,
        );

        let source = Json::file("config.json");
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("hosts[0]").map(|v| v.value.as_str()),
            Some(Some("host1"))
        );
        assert_eq!(
            values.get("hosts.__len").map(|v| v.value.as_integer()),
            Some(Some(3))
        );
    }

    #[test]
    fn test_json_null_values() {
        let env = MockEnv::new().with_file(
            "config.json",
            r#"{"optional_field": null, "required_field": "value"}"#,
        );

        let source = Json::file("config.json");
        let values = source.load(&env).expect("should load successfully");

        assert!(values.get("optional_field").is_some());
        // Null handling depends on implementation choice
    }

    #[test]
    fn test_json_parse_error_with_location() {
        let env = MockEnv::new().with_file(
            "config.json",
            r#"{"host": "localhost", "port": }"#,  // Invalid JSON
        );

        let source = Json::file("config.json");
        let result = source.load(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        match errors.first() {
            ConfigError::SourceError { kind, .. } => {
                assert!(matches!(
                    kind,
                    SourceErrorKind::ParseError { line: Some(_), column: Some(_), .. }
                ));
            }
            _ => panic!("Expected SourceError with ParseError"),
        }
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

        assert_eq!(values.get("string_val").map(|v| v.value.as_str()), Some(Some("hello")));
        assert_eq!(values.get("int_val").map(|v| v.value.as_integer()), Some(Some(42)));
        assert!(values.get("float_val").map(|v| v.value.as_float()).is_some());
        assert_eq!(values.get("bool_val").map(|v| v.value.as_bool()), Some(Some(true)));
    }

    #[test]
    fn test_json_string_source() {
        let env = MockEnv::new();
        let source = Json::string(r#"{"host": "localhost"}"#);
        let values = source.load(&env).expect("should load successfully");

        assert_eq!(
            values.get("host").map(|v| v.value.as_str()),
            Some(Some("localhost"))
        );
    }
}
```

### Integration Tests

- Test JSON source with full `Config` builder
- Test layering JSON with other sources (TOML, Env, Defaults)
- Test validation on JSON-loaded config
- Test tracing with JSON source

## Documentation Requirements

- **Code Documentation**: Doc comments with examples on all public APIs
- **User Documentation**: Update CLAUDE.md with JSON examples
- **Feature Flag Documentation**: Document the `json` feature in Cargo.toml

## Implementation Notes

- Follow the exact same structure as `toml_source.rs` for consistency
- Reuse helper functions where possible (e.g., `flatten_value` pattern)
- JSON numbers without decimals should be `Value::Integer`, with decimals `Value::Float`
- Large integers that don't fit in i64 should be handled gracefully
- Consider JSON5 support as a future enhancement (separate spec)

## Migration and Compatibility

Not applicable - new feature. Existing code unaffected when feature disabled.
