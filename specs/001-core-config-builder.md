---
number: 1
title: Core Configuration Builder
category: foundation
priority: critical
status: draft
dependencies: []
created: 2025-11-25
---

# Specification 001: Core Configuration Builder

**Category**: foundation
**Priority**: critical
**Status**: draft
**Dependencies**: None (foundation spec)

## Context

The premortem library needs a core configuration system that loads configuration from multiple sources and merges them by priority. This is the foundational component upon which all other features are built. The builder pattern provides a fluent API for constructing configuration instances.

Unlike traditional configuration libraries that fail on the first error, premortem accumulates all errors using the `Validation` type from the stillwater crate, providing a "premortem" experience where users see all configuration problems at once.

## Objective

Implement the core `Config<T>` struct and `ConfigBuilder` that provides the foundational configuration loading and merging capability with full error accumulation.

## Requirements

### Functional Requirements

1. **Config Struct**: A generic `Config<T>` wrapper that holds validated configuration
2. **ConfigBuilder**: Builder pattern for registering sources and options
3. **Source Trait**: A trait that all configuration sources must implement
4. **Priority-based Merging**: Later sources override earlier sources
5. **Error Accumulation**: All errors collected into `Validation<T, Vec<ConfigError>>`
6. **Path-based Access**: Internal representation using dot-notation paths (e.g., "database.host")

### Non-Functional Requirements

- Zero-cost abstractions where possible
- Clear, ergonomic API following Rust conventions
- Comprehensive error context for debugging
- Support for both sync and async source loading (feature-gated)

## Acceptance Criteria

- [ ] `Config<T>` struct exists and wraps validated configuration
- [ ] `ConfigBuilder` implements fluent builder pattern
- [ ] `Source` trait defined with `load()` method returning intermediate value representation
- [ ] Sources can be registered with `.source()` method
- [ ] Later sources override earlier sources during merge
- [ ] Build returns `Validation<Config<T>, Vec<ConfigError>>`
- [ ] Empty builder with no sources returns appropriate error
- [ ] Unit tests cover builder construction and source ordering

## Technical Details

### Core Structures

```rust
/// The main configuration container
pub struct Config<T> {
    value: T,
}

impl<T> Config<T> {
    /// Get a reference to the configuration value
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Consume and return the configuration value
    pub fn into_inner(self) -> T {
        self.value
    }
}

/// Builder for constructing configuration
pub struct ConfigBuilder<T> {
    sources: Vec<Box<dyn Source>>,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned + Validate> Config<T> {
    pub fn builder() -> ConfigBuilder<T> {
        ConfigBuilder::new()
    }
}

impl<T: DeserializeOwned + Validate> ConfigBuilder<T> {
    pub fn new() -> Self { ... }

    pub fn source<S: Source + 'static>(mut self, source: S) -> Self { ... }

    pub fn build(self) -> Validation<Config<T>, Vec<ConfigError>> { ... }
}
```

### Source Trait

```rust
/// Trait for configuration sources
pub trait Source: Send + Sync {
    /// Load configuration values from this source
    fn load(&self) -> Validation<ConfigValues, Vec<ConfigError>>;

    /// Human-readable name of this source for error messages
    fn name(&self) -> &str;
}

/// Intermediate representation of configuration values
pub struct ConfigValues {
    values: BTreeMap<String, ConfigValue>,
}

/// A single configuration value with source information
pub struct ConfigValue {
    value: Value,  // serde_json::Value or custom enum
    source: SourceLocation,
}
```

### Implementation Approach

1. Create `ConfigValues` as intermediate representation using a flat map with dot-notation keys
2. Implement merge logic that preserves source location for each value
3. Deserialize merged values into target type `T` using serde
4. Run validation on deserialized value
5. Collect all errors (deserialization + validation) into single result

### Architecture Changes

- New `config` module with `Config`, `ConfigBuilder`
- New `source` module with `Source` trait, `ConfigValues`, `ConfigValue`
- New `value` module for `Value` enum (or use serde_json::Value)

### Data Structures

```rust
/// Raw value representation
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
}

/// Location where a value originated
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub source: String,      // "config.toml", "env:APP_FOO", etc.
    pub line: Option<u32>,
    pub column: Option<u32>,
}
```

## Dependencies

- **Prerequisites**: None (foundation spec)
- **Affected Components**: This is new code
- **External Dependencies**:
  - `serde` for deserialization
  - `stillwater` for `Validation` type

## Testing Strategy

- **Unit Tests**:
  - Builder construction and method chaining
  - Source ordering and priority
  - Merge logic with overlapping keys
  - Empty builder error handling
- **Integration Tests**:
  - Full build cycle with mock sources
  - Error accumulation from multiple sources
- **Performance Tests**: Deferred to optimization phase

## Documentation Requirements

- **Code Documentation**: Doc comments on all public items
- **User Documentation**: Basic usage examples in module docs
- **Architecture Updates**: None yet (new project)

## Implementation Notes

- Use `serde_json::Value` initially for simplicity, consider custom `Value` type later
- The `Validate` trait is defined in spec 003
- Source implementations are in separate specs (004+)
- Consider making `Source::load` return `impl Future` for async support

## Migration and Compatibility

Not applicable - new project.
