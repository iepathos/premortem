---
number: 3
title: Validate Trait and Core Validators
category: foundation
priority: critical
status: draft
dependencies: [2]
created: 2025-11-25
---

# Specification 003: Validate Trait and Core Validators

**Category**: foundation
**Priority**: critical
**Status**: draft
**Dependencies**: [002 - Error Types]

## Context

The `Validate` trait is the core abstraction for validating configuration values. It integrates with stillwater's `Validation` type to accumulate all validation errors. This spec defines the trait itself and the built-in validators that can be used both programmatically and via the derive macro (spec 004).

### Stillwater Patterns Applied

This spec embodies stillwater's core principles:

1. **Pure Core** - All validators are pure functions: `&T -> Validation<(), ConfigErrors>`
2. **Fail Completely** - Use `Validation::all()` to accumulate ALL errors, not just the first
3. **Traverse** - Use `Validation::traverse()` for validating collections with error accumulation
4. **Semigroup Combination** - Errors combine via `ConfigErrors`'s `Semigroup` implementation
5. **Composition Over Complexity** - Small, focused validators that compose together

```
    Input Value
         │
         ▼
   ┌─────────────────────────────────────────┐
   │           Pure Validation Core          │
   │  (no I/O, just data transformation)     │
   │                                         │
   │   validator1 ──┐                        │
   │   validator2 ──┼── Validation::all() ──▶ Success | Failure(all errors)
   │   validator3 ──┘                        │
   └─────────────────────────────────────────┘
```

## Objective

Define the `Validate` trait and implement core validators for common validation patterns (non-empty, range, format, etc.) that return `ConfigValidation<()>` using stillwater's functional patterns.

## Requirements

### Functional Requirements

1. **Validate Trait**: Core trait with `validate(&self) -> ConfigValidation<()>`
2. **Built-in Validators**: Common validators for strings, numbers, paths, collections
3. **Validator Composition**: Combine validators using `Validation::all()`
4. **Path Context**: Validators receive the field path for error messages
5. **Nested Validation**: Support for validating nested structs
6. **Collection Validation**: Use `Validation::traverse()` for collections

### Non-Functional Requirements

- Validators should be pure functions (no I/O)
- All validators return `ConfigValidation<T>` for Semigroup-based accumulation
- Clear error messages with the field path
- Support for custom error messages
- Extensible for custom validators

## Acceptance Criteria

- [ ] `Validate` trait defined with `validate()` method returning `ConfigValidation<()>`
- [ ] `validate_field()` helper using `Validation::all()` for multiple validators
- [ ] String validators: `non_empty`, `min_length`, `max_length`, `pattern`, `email`, `url`
- [ ] Numeric validators: `range`, `positive`, `negative`, `non_zero`
- [ ] Collection validators using `Validation::traverse()`: `non_empty`, `min_length`, `max_length`, `each`
- [ ] Path validators: `file_exists`, `dir_exists`, `parent_exists`
- [ ] Nested struct validation support
- [ ] All validators return `ConfigValidation<()>` using `ConfigErrors` (NonEmptyVec)
- [ ] Unit tests for each validator including Semigroup combination

## Technical Details

### Validate Trait

```rust
use stillwater::Validation;
use crate::error::{ConfigError, ConfigErrors, ConfigValidation};

/// Trait for types that can be validated.
///
/// This is a pure validation trait - no I/O allowed.
/// Follows stillwater's "pure core" pattern.
pub trait Validate {
    /// Validate this value, returning all validation errors.
    ///
    /// Uses `ConfigValidation<()>` which wraps `Validation<(), ConfigErrors>`.
    /// ConfigErrors implements Semigroup, enabling error accumulation via `Validation::all()`.
    fn validate(&self) -> ConfigValidation<()>;

    /// Validate with a path prefix for error context.
    ///
    /// This adds context to all errors, following stillwater's error trail pattern.
    fn validate_at(&self, path: &str) -> ConfigValidation<()> {
        self.validate().map_err(|errors| errors.with_path_prefix(path))
    }
}

/// Blanket implementation for Option<T> where T: Validate.
/// None is always valid (use required field validation separately).
impl<T: Validate> Validate for Option<T> {
    fn validate(&self) -> ConfigValidation<()> {
        match self {
            Some(value) => value.validate(),
            None => Validation::Success(()),
        }
    }
}

/// Blanket implementation for Vec<T> where T: Validate.
/// Uses stillwater's traverse pattern to validate all items with error accumulation.
impl<T: Validate> Validate for Vec<T> {
    fn validate(&self) -> ConfigValidation<()> {
        // Use traverse to validate each item, accumulating all errors
        Validation::traverse(
            self.iter().enumerate(),
            |(i, item)| item.validate_at(&format!("[{}]", i))
        ).map(|_: Vec<()>| ())
    }
}
```

### Validator Types

```rust
use crate::error::{ConfigError, ConfigErrors, ConfigValidation};

/// A validator function that checks a value.
///
/// Validators are pure functions: `&T -> ConfigValidation<()>`
/// They return either Success(()) or Failure(ConfigErrors).
pub trait Validator<T: ?Sized> {
    fn validate(&self, value: &T, path: &str) -> ConfigValidation<()>;
}

/// Built-in validators.
///
/// All validators are zero-cost structs that implement the Validator trait.
/// They can be composed using Validation::all() for multiple checks on one field.
pub mod validators {
    use super::*;

    // String validators (pure, no I/O)
    pub struct NonEmpty;
    pub struct MinLength(pub usize);
    pub struct MaxLength(pub usize);
    pub struct Length(pub std::ops::RangeInclusive<usize>);
    pub struct Pattern(pub regex::Regex);
    pub struct Email;
    pub struct Url;

    // Numeric validators (pure, no I/O)
    pub struct Range<T>(pub std::ops::RangeInclusive<T>);
    pub struct Positive;
    pub struct Negative;
    pub struct NonZero;

    // Collection validators (pure, no I/O)
    // Uses traverse for error accumulation
    pub struct Each<V>(pub V);

    // Path validators
    // Note: file_exists/dir_exists check filesystem - consider Effect for production
    pub struct FileExists;
    pub struct DirExists;
    pub struct ParentExists;
    pub struct Extension(pub String);
}
```

### Validator Implementations

```rust
use stillwater::Validation;
use crate::error::{ConfigError, ConfigErrors, ConfigValidation};

/// Helper to create a validation failure with a single error
fn fail(error: ConfigError) -> ConfigValidation<()> {
    Validation::Failure(ConfigErrors::single(error))
}

impl Validator<str> for NonEmpty {
    fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
        if value.is_empty() {
            fail(ConfigError::ValidationError {
                path: path.to_string(),
                source_location: None,
                value: Some(String::new()),
                message: "value cannot be empty".to_string(),
            })
        } else {
            Validation::Success(())
        }
    }
}

impl Validator<str> for MinLength {
    fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
        if value.len() < self.0 {
            fail(ConfigError::ValidationError {
                path: path.to_string(),
                source_location: None,
                value: Some(value.to_string()),
                message: format!("length {} is less than minimum {}", value.len(), self.0),
            })
        } else {
            Validation::Success(())
        }
    }
}

impl<T: PartialOrd + std::fmt::Display + Clone> Validator<T> for Range<T> {
    fn validate(&self, value: &T, path: &str) -> ConfigValidation<()> {
        if !self.0.contains(value) {
            fail(ConfigError::ValidationError {
                path: path.to_string(),
                source_location: None,
                value: Some(value.to_string()),
                message: format!(
                    "value {} is not in range {}..={}",
                    value, self.0.start(), self.0.end()
                ),
            })
        } else {
            Validation::Success(())
        }
    }
}

/// Note: FileExists performs I/O (filesystem check).
/// For strict pure core / imperative shell, consider using Effect.
/// Kept here for convenience in config validation at startup.
impl Validator<std::path::Path> for FileExists {
    fn validate(&self, value: &std::path::Path, path: &str) -> ConfigValidation<()> {
        if !value.is_file() {
            fail(ConfigError::ValidationError {
                path: path.to_string(),
                source_location: None,
                value: Some(value.display().to_string()),
                message: "file does not exist".to_string(),
            })
        } else {
            Validation::Success(())
        }
    }
}

/// Each validator - validates every item in a collection using traverse.
/// Accumulates ALL errors across ALL items.
impl<V, T> Validator<[T]> for Each<V>
where
    V: Validator<T>,
{
    fn validate(&self, value: &[T], path: &str) -> ConfigValidation<()> {
        Validation::traverse(
            value.iter().enumerate(),
            |(i, item)| self.0.validate(item, &format!("{}[{}]", path, i))
        ).map(|_: Vec<()>| ())
    }
}
```

### Field Validation Helper

```rust
use stillwater::Validation;
use crate::error::{ConfigErrors, ConfigValidation};

/// Validate a field against multiple validators using Validation::all().
///
/// This is the core composition pattern - run all validators and accumulate errors.
/// Follows stillwater's "fail completely" philosophy.
pub fn validate_field<T>(
    value: &T,
    path: &str,
    validators: &[&dyn Validator<T>],
) -> ConfigValidation<()>
where
    T: ?Sized,
{
    // Collect all validation results
    let results: Vec<ConfigValidation<()>> = validators
        .iter()
        .map(|v| v.validate(value, path))
        .collect();

    // Use Validation::all() to combine - accumulates ALL errors via Semigroup
    Validation::all(results).map(|_| ())
}

/// Validate a nested struct with path context.
pub fn validate_nested<T: Validate>(
    value: &T,
    path: &str,
) -> ConfigValidation<()> {
    value.validate_at(path)
}

/// Validate an optional nested struct.
/// None is always valid.
pub fn validate_optional_nested<T: Validate>(
    value: &Option<T>,
    path: &str,
) -> ConfigValidation<()> {
    match value {
        Some(v) => v.validate_at(path),
        None => Validation::Success(()),
    }
}
```

### Custom Validator Support

```rust
/// Create a custom validator from a pure function.
///
/// # Example
/// ```rust
/// let port_validator = custom(|port: &u16, path: &str| {
///     if *port == 0 {
///         fail(ConfigError::ValidationError {
///             path: path.into(),
///             message: "port cannot be 0".into(),
///             ..Default::default()
///         })
///     } else {
///         Validation::Success(())
///     }
/// });
/// ```
pub fn custom<T, F>(f: F) -> impl Validator<T>
where
    F: Fn(&T, &str) -> ConfigValidation<()>,
{
    struct Custom<F>(F);

    impl<T, F> Validator<T> for Custom<F>
    where
        F: Fn(&T, &str) -> ConfigValidation<()>,
    {
        fn validate(&self, value: &T, path: &str) -> ConfigValidation<()> {
            (self.0)(value, path)
        }
    }

    Custom(f)
}
```

### Conditional Validation

```rust
/// Validator that only runs when a condition is true.
///
/// Useful for "when X is set, Y must be valid" patterns.
pub struct When<V, F> {
    validator: V,
    condition: F,
}

impl<V, F> When<V, F> {
    pub fn new(validator: V, condition: F) -> Self {
        Self { validator, condition }
    }
}

impl<V, F, T> Validator<T> for When<V, F>
where
    V: Validator<T>,
    F: Fn() -> bool,
{
    fn validate(&self, value: &T, path: &str) -> ConfigValidation<()> {
        if (self.condition)() {
            self.validator.validate(value, path)
        } else {
            Validation::Success(())
        }
    }
}
```

### Complete Example: Combining Validators

```rust
use stillwater::Validation;
use crate::error::{ConfigError, ConfigErrors, ConfigValidation};
use crate::validators::*;

/// Example: Validating a complete config struct using stillwater patterns.
///
/// This shows:
/// 1. Pure validation (no I/O in the core)
/// 2. Validation::all() for combining independent validations
/// 3. Nested validation with path context
/// 4. Error accumulation via ConfigErrors Semigroup
#[derive(Debug)]
struct DatabaseConfig {
    host: String,
    port: u16,
    pool_size: u32,
    replica: Option<ReplicaConfig>,
}

impl Validate for DatabaseConfig {
    fn validate(&self) -> ConfigValidation<()> {
        // Use Validation::all() to run ALL validations and accumulate errors
        Validation::all((
            // Field validations (pure)
            validate_field(&self.host, "host", &[&NonEmpty]),
            validate_field(&self.port, "port", &[&Range(1..=65535)]),
            validate_field(&self.pool_size, "pool_size", &[&Range(1..=100)]),

            // Nested validation with path context
            validate_optional_nested(&self.replica, "replica"),

            // Cross-field validation (pure)
            self.validate_replica_not_same_as_primary(),
        ))
        .map(|_| ())
    }
}

impl DatabaseConfig {
    /// Cross-field validation: replica can't be same as primary.
    /// This is a pure function - no I/O.
    fn validate_replica_not_same_as_primary(&self) -> ConfigValidation<()> {
        if let Some(replica) = &self.replica {
            if replica.host == self.host && replica.port == self.port {
                return Validation::Failure(ConfigErrors::single(
                    ConfigError::CrossFieldError {
                        paths: vec!["host".into(), "port".into(), "replica".into()],
                        message: "replica cannot be same as primary".into(),
                    }
                ));
            }
        }
        Validation::Success(())
    }
}
```

## Dependencies

- **Prerequisites**: Spec 002 (Error Types)
- **Affected Components**: Used by derive macro (004), all config types
- **External Dependencies**:
  - `stillwater` for `Validation` type
  - `regex` for pattern validation
  - `url` crate for URL validation

## Testing Strategy

- **Unit Tests**:
  - Each validator in isolation
  - Validator composition
  - Nested struct validation
  - Option and Vec validation
  - Custom validators
- **Integration Tests**: Combined with derive macro tests

## Documentation Requirements

- **Code Documentation**: Doc comments with examples for each validator
- **User Documentation**: Validation guide with common patterns

## Implementation Notes

- Consider using `once_cell` for compiled regex patterns
- Email validation should be RFC 5322 compliant (consider `email_address` crate)
- URL validation should use the `url` crate's parser
- Path validators should work with both `Path` and `PathBuf`
- Consider adding `IpAddr`, `SocketAddr`, `Uuid` validators

## Migration and Compatibility

Not applicable - new project.
