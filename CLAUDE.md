# Premortem Project Documentation

This document contains Premortem-specific documentation for Claude. General development guidelines are in `~/.claude/CLAUDE.md`.

## Overview

Premortem is a configuration library that performs a premortem on your app's config—finding all the ways it could die from bad config before it ever runs. It uses stillwater's functional patterns for error accumulation and composable validation.

## Error Handling

### Core Rules
- **Production code**: Never use `unwrap()` or `panic!()` - use Result types and `?` operator
- **Test code**: May use `unwrap()` and `panic!()` for test failures
- **Static patterns**: Compile-time constants (like regex) may use `expect()`

### Error Types
- Configuration: `ConfigError`, `ConfigErrors` (NonEmptyVec-based)
- Validation: `ConfigValidation<T>` (alias for `Validation<T, ConfigErrors>`)
- Source errors: `SourceErrorKind`

### Error Accumulation
Premortem uses stillwater's `Validation` type to collect ALL configuration errors, not just the first:

```rust
use premortem::{ConfigError, ConfigErrors, ConfigValidation};
use stillwater::Validation;

fn validate_config(config: &AppConfig) -> ConfigValidation<()> {
    let mut errors = Vec::new();

    if config.port == 0 {
        errors.push(ConfigError::ValidationError {
            path: "port".to_string(),
            source_location: None,
            value: Some("0".to_string()),
            message: "port must be non-zero".to_string(),
        });
    }

    if config.host.is_empty() {
        errors.push(ConfigError::ValidationError {
            path: "host".to_string(),
            source_location: None,
            value: Some("".to_string()),
            message: "host cannot be empty".to_string(),
        });
    }

    match ConfigErrors::from_vec(errors) {
        Some(errs) => Validation::Failure(errs),
        None => Validation::Success(()),
    }
}
```

**Benefits**: Users see ALL config problems at once, not one at a time.

## Architecture

### Pure Core, Imperative Shell

Premortem follows stillwater's architectural pattern:

- **Pure Core**: Value merging, deserialization, validation are pure functions
- **Imperative Shell**: I/O operations use the `ConfigEnv` trait for dependency injection

```
src/
├── config.rs    # Config and ConfigBuilder (builder pattern)
├── error.rs     # ConfigError, ConfigErrors, ConfigValidation
├── value.rs     # Value enum for intermediate representation
├── source.rs    # Source trait and ConfigValues container
├── env.rs       # ConfigEnv trait and MockEnv for testing
└── validate.rs  # Validate trait for custom validation
```

### ConfigEnv Trait (Testable I/O)

All I/O is abstracted through the `ConfigEnv` trait:

```rust
use premortem::{Config, ConfigEnv, MockEnv, RealEnv};

// Production: uses real file system and environment
let config = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .build();  // Uses RealEnv internally

// Testing: uses mock file system and environment
let env = MockEnv::new()
    .with_file("config.toml", r#"host = "localhost"\nport = 8080"#)
    .with_env("APP_DEBUG", "true");

let config = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .build_with_env(&env);
```

## Config Builder Pattern

### Basic Usage

```rust
use premortem::{Config, Validate, ConfigValidation};
use serde::Deserialize;
use stillwater::Validation;

#[derive(Debug, Deserialize)]
struct AppConfig {
    host: String,
    port: u16,
}

impl Validate for AppConfig {
    fn validate(&self) -> ConfigValidation<()> {
        if self.port > 0 {
            Validation::Success(())
        } else {
            Validation::fail_with(ConfigError::ValidationError {
                path: "port".to_string(),
                source_location: None,
                value: Some(self.port.to_string()),
                message: "port must be positive".to_string(),
            })
        }
    }
}

let config = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .source(Env::new().prefix("APP"))
    .build()?;
```

### Source Layering

Sources are applied in order, with later sources overriding earlier ones:

```rust
// 1. Base defaults
// 2. File config overrides defaults
// 3. Environment variables override file config
let config = Config::<AppConfig>::builder()
    .source(Defaults::new())
    .source(Toml::file("config.toml"))
    .source(Env::new().prefix("APP"))
    .build()?;
```

## Error Types Reference

### ConfigError Variants

| Variant | Description |
|---------|-------------|
| `SourceError` | A configuration source failed to load |
| `ParseError` | A field failed to parse to expected type |
| `MissingField` | A required field is missing |
| `ValidationError` | A validation rule failed |
| `CrossFieldError` | Cross-field validation failed |
| `UnknownField` | An unknown field was found |
| `NoSources` | No sources were provided to the builder |

### SourceErrorKind Variants

| Variant | Description |
|---------|-------------|
| `NotFound` | Source file was not found |
| `IoError` | Source file could not be read |
| `ParseError` | Source content could not be parsed |
| `Other` | Other source-specific error |

## Feature Flags

```toml
[features]
default = ["toml", "derive"]
toml = []        # TOML file support
json = []        # JSON file support
watch = []       # File watching for hot reload
derive = []      # Derive macro for Validate trait
full = ["toml", "json", "watch", "derive"]
```

## Testing Patterns

### MockEnv for Unit Tests

```rust
#[test]
fn test_config_loading() {
    let env = MockEnv::new()
        .with_file("config.toml", r#"
            [database]
            host = "localhost"
            port = 5432
        "#)
        .with_env("APP_DATABASE_HOST", "prod-db.example.com");

    let config = Config::<DbConfig>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::new().prefix("APP"))
        .build_with_env(&env);

    assert!(config.is_ok());
    let config = config.unwrap();
    // Environment override wins
    assert_eq!(config.database.host, "prod-db.example.com");
}
```

### Testing Validation

```rust
#[test]
fn test_validation_accumulates_errors() {
    let env = MockEnv::new()
        .with_file("config.toml", r#"
            host = ""
            port = 0
        "#);

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    // Both validation errors should be present
    assert_eq!(errors.len(), 2);
}
```

### Testing Error Scenarios

```rust
#[test]
fn test_permission_denied() {
    let env = MockEnv::new()
        .with_unreadable_file("secrets.toml");

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("secrets.toml"))
        .build_with_env(&env);

    assert!(result.is_err());
    // Check for SourceError with IoError kind
}

#[test]
fn test_missing_file() {
    let env = MockEnv::new()
        .with_missing_file("config.toml");

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    assert!(result.is_err());
    // Check for SourceError with NotFound kind
}
```

## Stillwater Integration

Premortem uses these stillwater types:

| Type | Usage |
|------|-------|
| `Validation<T, E>` | Error accumulation for config errors |
| `NonEmptyVec<T>` | Guaranteed non-empty error lists |
| `Semigroup` | Combining errors from multiple sources |

### Re-exports

For convenience, premortem re-exports commonly used stillwater types:

```rust
pub use stillwater::{NonEmptyVec, Semigroup, Validation};
```

## Common Patterns

### Custom Validation

```rust
impl Validate for DatabaseConfig {
    fn validate(&self) -> ConfigValidation<()> {
        use stillwater::Validation;

        let port_valid = if self.port > 0 && self.port < 65536 {
            Validation::Success(())
        } else {
            Validation::Failure(ConfigErrors::single(
                ConfigError::ValidationError { /* ... */ }
            ))
        };

        let host_valid = if !self.host.is_empty() {
            Validation::Success(())
        } else {
            Validation::Failure(ConfigErrors::single(
                ConfigError::ValidationError { /* ... */ }
            ))
        };

        // Combine validations - accumulates ALL errors
        port_valid.combine_with(host_valid, |_, _| ())
    }
}
```

### Source Location Tracking

```rust
// Track where config values came from
let loc = SourceLocation::new("config.toml")
    .with_line(10)
    .with_column(5);

// For environment variables
let loc = SourceLocation::env("APP_HOST");

// In error messages: "[config.toml:10:5] 'port': expected integer"
```

### Error Grouping

```rust
use premortem::group_by_source;

let errors: ConfigErrors = /* ... */;
let grouped = group_by_source(&errors);

for (source, errs) in grouped {
    println!("Errors in {}:", source);
    for err in errs {
        println!("  {}", err);
    }
}
```

## Development Commands

```bash
# Run tests
cargo test

# Run tests with all features
cargo test --all-features

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy --all-features

# Build docs
cargo doc --all-features --no-deps
```

## Module Dependencies

```
stillwater (functional patterns)
    ↓
premortem
├── error.rs (uses Validation, NonEmptyVec, Semigroup)
├── config.rs (uses error, source, validate, env)
├── source.rs (uses value, error, env)
├── value.rs (standalone)
├── validate.rs (uses error)
└── env.rs (standalone)
```
