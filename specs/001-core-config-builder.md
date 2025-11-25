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

### Stillwater Architecture Pattern

This spec implements stillwater's **"Pure Core, Imperative Shell"** pattern:

```
┌─────────────────────────────────────────────────────────────────┐
│                    IMPERATIVE SHELL (I/O)                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │ TOML Source │  │ Env Source  │  │ Defaults    │  (Effects)   │
│  │  (file I/O) │  │ (env vars)  │  │ (pure)      │              │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘              │
│         │                │                │                      │
│         └────────────────┼────────────────┘                      │
│                          ▼                                       │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                    PURE CORE                             │    │
│  │                                                          │    │
│  │   ConfigValues ──► Merge ──► Deserialize ──► Validate   │    │
│  │                         (all pure functions)             │    │
│  │                                                          │    │
│  │   Returns: Validation<Config<T>, ConfigErrors>          │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

**Key Design Decisions:**
1. **Sources use `Effect`** - File/network I/O wrapped in stillwater's Effect type
2. **Merging is pure** - No I/O, just data transformation
3. **Validation is pure** - Uses `Validation::all()` for error accumulation
4. **Errors use `ConfigErrors`** - NonEmptyVec with Semigroup for combination

## Objective

Implement the core `Config<T>` struct and `ConfigBuilder` that provides the foundational configuration loading and merging capability with full error accumulation, following stillwater's pure core / imperative shell pattern.

## Requirements

### Functional Requirements

1. **Config Struct**: A generic `Config<T>` wrapper that holds validated configuration
2. **ConfigBuilder**: Builder pattern for registering sources and options
3. **Source Trait**: A trait using stillwater's `Effect` for I/O operations
4. **Priority-based Merging**: Pure function - later sources override earlier sources
5. **Error Accumulation**: All errors collected into `Validation<T, ConfigErrors>` (NonEmptyVec)
6. **Path-based Access**: Internal representation using dot-notation paths (e.g., "database.host")

### Non-Functional Requirements

- Clear separation between I/O (Effect) and pure logic (Validation)
- Zero-cost abstractions where possible
- Clear, ergonomic API following Rust conventions
- Comprehensive error context with stillwater's `.context()` pattern
- Support for both sync and async source loading via Effect

## Acceptance Criteria

- [ ] `Config<T>` struct exists and wraps validated configuration
- [ ] `ConfigBuilder` implements fluent builder pattern
- [ ] `Source` trait defined with `load()` returning `Effect<ConfigValues, ConfigErrors, ()>`
- [ ] Sources can be registered with `.source()` method
- [ ] Merging is a pure function: `Vec<ConfigValues> -> ConfigValues`
- [ ] Build returns `ConfigValidation<Config<T>>` (using ConfigErrors with Semigroup)
- [ ] Error accumulation uses `Validation::all()` pattern
- [ ] Empty builder with no sources returns appropriate error
- [ ] Unit tests cover builder construction, source ordering, and error accumulation

## Technical Details

### Core Structures

```rust
use stillwater::{Effect, Validation};
use crate::error::{ConfigError, ConfigErrors, ConfigValidation};

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

/// Builder for constructing configuration.
///
/// Follows stillwater's composition pattern:
/// - Register sources (I/O effects)
/// - Build executes effects and runs pure validation
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

    /// Add a configuration source.
    /// Sources are loaded in order; later sources override earlier ones.
    pub fn source<S: Source + 'static>(mut self, source: S) -> Self { ... }

    /// Build the configuration.
    ///
    /// This is where the I/O happens (imperative shell).
    /// Returns Validation with ConfigErrors (NonEmptyVec with Semigroup).
    pub fn build(self) -> ConfigValidation<Config<T>> { ... }
}
```

### Source Trait (Effect-based)

```rust
use stillwater::Effect;
use crate::error::{ConfigErrors, ConfigValidation};

/// Trait for configuration sources.
///
/// Sources perform I/O via stillwater's Effect type.
/// This keeps I/O at the boundaries (imperative shell).
pub trait Source: Send + Sync {
    /// Load configuration values from this source.
    ///
    /// Uses Effect for I/O operations (file reads, env vars, network).
    /// The unit environment `()` means no runtime dependencies.
    fn load(&self) -> Effect<ConfigValues, ConfigErrors, ()>;

    /// Human-readable name of this source for error messages
    fn name(&self) -> &str;

    /// Optional: path to watch for hot reload (if applicable)
    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        None
    }
}

/// Intermediate representation of configuration values.
/// This is the "data" that flows through the pure core.
pub struct ConfigValues {
    values: BTreeMap<String, ConfigValue>,
}

impl ConfigValues {
    pub fn empty() -> Self {
        Self { values: BTreeMap::new() }
    }

    pub fn insert(&mut self, path: String, value: ConfigValue) {
        self.values.insert(path, value);
    }

    pub fn get(&self, path: &str) -> Option<&ConfigValue> {
        self.values.get(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &ConfigValue)> {
        self.values.iter()
    }
}

/// A single configuration value with source information
pub struct ConfigValue {
    pub value: Value,
    pub source: SourceLocation,
}
```

### Implementation Approach (Pure Core / Imperative Shell)

```rust
impl<T: DeserializeOwned + Validate> ConfigBuilder<T> {
    /// Build configuration following stillwater's patterns.
    pub fn build(self) -> ConfigValidation<Config<T>> {
        // IMPERATIVE SHELL: Execute all source Effects to get ConfigValues
        let source_results: Vec<Result<ConfigValues, ConfigErrors>> = self.sources
            .iter()
            .map(|source| {
                source.load()
                    .context(format!("Loading {}", source.name()))
                    .run(&())  // Execute the Effect
            })
            .collect();

        // Convert Results to Validations for error accumulation
        let validations: Vec<ConfigValidation<ConfigValues>> = source_results
            .into_iter()
            .map(|r| match r {
                Ok(values) => Validation::Success(values),
                Err(errors) => Validation::Failure(errors),
            })
            .collect();

        // PURE CORE: Accumulate source loading errors
        let all_values: ConfigValidation<Vec<ConfigValues>> =
            Validation::all(validations);

        // PURE CORE: Merge, deserialize, and validate
        all_values.and_then(|values_vec| {
            // Pure merge function
            let merged = merge_config_values(values_vec);

            // Pure deserialization
            deserialize_config::<T>(&merged)
                .and_then(|config| {
                    // Pure validation using Validation::all()
                    config.validate()
                        .map(|_| Config { value: config })
                })
        })
    }
}

/// Pure function: merge multiple ConfigValues by priority.
/// Later values override earlier values.
fn merge_config_values(all_values: Vec<ConfigValues>) -> ConfigValues {
    let mut merged = ConfigValues::empty();

    for values in all_values {
        for (path, value) in values.iter() {
            merged.insert(path.clone(), value.clone());
        }
    }

    merged
}

/// Pure function: deserialize ConfigValues into target type.
fn deserialize_config<T: DeserializeOwned>(values: &ConfigValues) -> ConfigValidation<T> {
    // Convert to serde_json::Value and deserialize
    let json = config_values_to_json(values);

    match serde_json::from_value::<T>(json) {
        Ok(config) => Validation::Success(config),
        Err(e) => Validation::Failure(ConfigErrors::single(
            ConfigError::ParseError {
                path: e.path().to_string(),
                source_location: SourceLocation::new("deserialization"),
                expected_type: std::any::type_name::<T>().to_string(),
                actual_value: "...".to_string(),
                message: e.to_string(),
            }
        )),
    }
}
```

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
  - `stillwater` for `Validation`, `Effect`, `Semigroup`, `NonEmptyVec`
  - `serde` for deserialization
  - `serde_json` for intermediate value representation

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
