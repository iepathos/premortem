---
number: 5
title: TOML File Source
category: storage
priority: high
status: draft
dependencies: [1, 2, 12]
created: 2025-11-25
---

# Specification 005: TOML File Source

**Category**: storage
**Priority**: high
**Status**: draft
**Dependencies**: [001 - Core Config Builder, 002 - Error Types, 012 - ConfigEnv Trait]

## Context

TOML is the default configuration format for Rust applications. The TOML source implementation provides file-based configuration loading with proper error reporting including line/column information for parse errors.

### Stillwater Pattern: Effect for I/O

File reading is I/O, which lives in the **imperative shell** per stillwater's architecture:

```
┌─────────────────────────────────────────┐
│  Toml::load() -> Effect<ConfigValues>   │  ← I/O at boundary
└───────────────────┬─────────────────────┘
                    │ (runs via .run(&()))
                    ▼
┌─────────────────────────────────────────┐
│  parse_toml() -> ConfigValidation       │  ← Pure parsing
│  flatten_value() -> ConfigValues        │  ← Pure transformation
└─────────────────────────────────────────┘
```

The `Source::load()` returns `Effect<ConfigValues, ConfigErrors, ()>`:
- **Effect** wraps the I/O operation (file read)
- **ConfigErrors** (NonEmptyVec) enables Semigroup error accumulation
- **Unit env `()`** means no runtime dependencies needed

## Objective

Implement a `Toml` source that loads configuration from TOML files using stillwater's Effect pattern for I/O, with support for required/optional files, string content, and rich error reporting with source locations.

## Requirements

### Functional Requirements

1. **File Loading**: Load TOML from file path
2. **Optional Files**: Support optional files that don't error if missing
3. **String Content**: Support loading from TOML string
4. **Line/Column Tracking**: Parse errors include exact position
5. **Nested Structure**: Flatten to dot-notation paths
6. **Source Attribution**: Each value tracks its source location

### Non-Functional Requirements

- Fail fast on file read errors (unless optional)
- Preserve comments metadata (for future tooling)
- Efficient parsing using `toml` crate

## Acceptance Criteria

- [ ] `Toml::file(path)` loads from file path
- [ ] `Toml::file(path).optional()` doesn't error if file missing
- [ ] `Toml::file(path).required()` explicitly marks as required
- [ ] `Toml::string(content)` loads from string
- [ ] Parse errors include line and column numbers
- [ ] Missing file error is clear and actionable
- [ ] Nested tables flatten to dot-notation (e.g., `database.host`)
- [ ] Arrays are preserved and properly indexed
- [ ] Integration test with realistic TOML config
- [ ] Unit tests for all methods and error cases

## Technical Details

### API Design

```rust
/// TOML configuration source
pub struct Toml {
    source: TomlSource,
    required: bool,
    name: Option<String>,
}

enum TomlSource {
    File(PathBuf),
    String { content: String, name: String },
}

impl Toml {
    /// Load TOML from a file path (required by default)
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            source: TomlSource::File(path.into()),
            required: true,
            name: None,
        }
    }

    /// Load TOML from a string
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

    /// Mark this source as optional (no error if file missing)
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// Mark this source as required (default)
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set a custom name for this source in error messages
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}
```

### Source Implementation (Effect-based with ConfigEnv)

```rust
use stillwater::Effect;
use crate::env::ConfigEnv;
use crate::error::{ConfigError, ConfigErrors, SourceErrorKind};
use crate::source::{ConfigValues, Source};

impl Source for Toml {
    /// Load TOML configuration wrapped in Effect.
    ///
    /// File I/O is performed through the `ConfigEnv` trait, enabling
    /// dependency injection for testing. Parsing is pure and happens
    /// after the I/O completes.
    ///
    /// # Stillwater Pattern
    ///
    /// - `Effect<T, E, Env>` defers I/O until `run()` is called
    /// - `ConfigEnv` parameter enables `MockEnv` injection for tests
    /// - Parsing remains pure (no I/O in `parse_toml`)
    fn load<E: ConfigEnv>(&self) -> Effect<ConfigValues, ConfigErrors, E> {
        let source = self.source.clone();
        let required = self.required;
        let custom_name = self.name.clone();

        // Wrap I/O in Effect with environment injection
        Effect::from_fn(move |env: &E| {
            let (content, source_name) = match &source {
                TomlSource::File(path) => {
                    // I/O through injected ConfigEnv (mockable!)
                    match env.read_file(path) {
                        Ok(content) => {
                            let name = custom_name.clone()
                                .unwrap_or_else(|| path.display().to_string());
                            (content, name)
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            if required {
                                return Err(ConfigErrors::single(ConfigError::SourceError {
                                    source_name: path.display().to_string(),
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
                                source_name: path.display().to_string(),
                                kind: SourceErrorKind::IoError {
                                    message: e.to_string(),
                                },
                            }));
                        }
                    }
                }
                TomlSource::String { content, name } => {
                    // String content - no I/O needed
                    (content.clone(), custom_name.clone().unwrap_or_else(|| name.clone()))
                }
            };

            // Pure parsing (after I/O)
            parse_toml(&content, &source_name)
        })
    }

    fn name(&self) -> &str {
        match &self.source {
            TomlSource::File(path) => self.name.as_ref()
                .map(String::as_str)
                .unwrap_or_else(|| path.to_str().unwrap_or("<file>")),
            TomlSource::String { name, .. } => self.name.as_ref()
                .map(String::as_str)
                .unwrap_or(name),
        }
    }

    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        match &self.source {
            TomlSource::File(path) => Some(path.clone()),
            TomlSource::String { .. } => None,
        }
    }
}
```

### TOML Parsing (Pure Function)

```rust
use crate::error::{ConfigError, ConfigErrors, SourceErrorKind};

/// Pure function: parse TOML content into ConfigValues.
/// No I/O - this runs after the Effect has read the file.
fn parse_toml(content: &str, source_name: &str) -> Result<ConfigValues, ConfigErrors> {
    // Use toml crate with span information
    let document: toml::de::Value = match content.parse() {
        Ok(doc) => doc,
        Err(e) => {
            return Err(ConfigErrors::single(ConfigError::SourceError {
                source_name: source_name.to_string(),
                kind: SourceErrorKind::ParseError {
                    message: e.message().to_string(),
                    line: e.line(),
                    column: e.column(),
                },
            }));
        }
    };

    // Pure transformation: TOML -> ConfigValues
    let mut values = ConfigValues::new();
    flatten_value(&document, "", source_name, &mut values);
    Ok(values)
}

/// Pure function: recursively flatten TOML structure to dot-notation paths.
/// This is a pure transformation - no I/O.
fn flatten_value(
    value: &toml::Value,
    prefix: &str,
    source_name: &str,
    values: &mut ConfigValues,
) {
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
                ConfigValue {
                    value: Value::Integer(arr.len() as i64),
                    source: SourceLocation::new(source_name),
                },
            );
        }
        _ => {
            let config_value = ConfigValue {
                value: toml_to_value(value),
                source: SourceLocation::new(source_name),
                // TODO: Track line numbers per value using toml_edit
            };
            values.insert(prefix.to_string(), config_value);
        }
    }
}

fn toml_to_value(toml: &toml::Value) -> Value {
    match toml {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(i) => Value::Integer(*i),
        toml::Value::Float(f) => Value::Float(*f),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(arr) => Value::Array(arr.iter().map(toml_to_value).collect()),
        toml::Value::Table(t) => {
            Value::Table(t.iter().map(|(k, v)| (k.clone(), toml_to_value(v))).collect())
        }
    }
}
```

### Line Number Tracking

For precise line numbers per value, consider using `toml_edit` crate:

```rust
// Alternative implementation using toml_edit for span information
fn parse_with_spans(content: &str, source_name: &str) -> Validation<ConfigValues, Vec<ConfigError>> {
    use toml_edit::{Document, Item};

    let doc: Document = match content.parse() {
        Ok(d) => d,
        Err(e) => { /* ... */ }
    };

    fn visit_item(
        item: &Item,
        prefix: &str,
        source_name: &str,
        values: &mut ConfigValues,
    ) {
        if let Some(span) = item.span() {
            // Calculate line number from span
            let line = content[..span.start].lines().count() as u32;
            // ... store with line number
        }
    }

    // ... walk document
}
```

## Dependencies

- **Prerequisites**: Specs 001, 002
- **Affected Components**: Config builder integration
- **External Dependencies**:
  - `stillwater` crate for `Effect` type
  - `toml` crate for parsing
  - `toml_edit` crate for span information (optional, for precise line tracking)

## Testing Strategy

- **Unit Tests**:
  - Valid TOML parsing
  - Parse error with line/column
  - Missing file (required vs optional)
  - Nested table flattening
  - Array handling
  - String source
- **Integration Tests**:
  - Full config load from file
  - Multi-source with TOML

## Documentation Requirements

- **Code Documentation**: Doc comments with TOML examples
- **User Documentation**: TOML configuration guide

## Implementation Notes

- Start with `toml` crate, add `toml_edit` for line tracking later
- TOML datetime should convert to string or custom type
- Consider preserving comments for future "config doctor" tooling
- Unicode in keys should work correctly

## Migration and Compatibility

Not applicable - new project.
