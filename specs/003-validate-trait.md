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

## Objective

Define the `Validate` trait and implement core validators for common validation patterns (non-empty, range, format, etc.) that return `Validation<(), Vec<ConfigError>>`.

## Requirements

### Functional Requirements

1. **Validate Trait**: Core trait with `validate(&self) -> Validation<(), Vec<ConfigError>>`
2. **Built-in Validators**: Common validators for strings, numbers, paths, collections
3. **Validator Composition**: Ability to combine multiple validators
4. **Path Context**: Validators receive the field path for error messages
5. **Nested Validation**: Support for validating nested structs

### Non-Functional Requirements

- Validators should be reusable and composable
- Clear error messages with the field path
- Support for custom error messages
- Extensible for custom validators

## Acceptance Criteria

- [ ] `Validate` trait defined with `validate()` method
- [ ] `validate_field()` helper for field-level validation
- [ ] String validators: `non_empty`, `min_length`, `max_length`, `pattern`, `email`, `url`
- [ ] Numeric validators: `range`, `positive`, `negative`, `non_zero`
- [ ] Collection validators: `non_empty`, `min_length`, `max_length`, `each`
- [ ] Path validators: `file_exists`, `dir_exists`, `parent_exists`
- [ ] Nested struct validation support
- [ ] All validators return `Validation<(), Vec<ConfigError>>`
- [ ] Unit tests for each validator

## Technical Details

### Validate Trait

```rust
use stillwater::Validation;

/// Trait for types that can be validated
pub trait Validate {
    /// Validate this value, returning all validation errors
    fn validate(&self) -> Validation<(), Vec<ConfigError>>;

    /// Validate with a path prefix for error context
    fn validate_at(&self, path: &str) -> Validation<(), Vec<ConfigError>> {
        self.validate().map_err(|errors| {
            errors.into_iter()
                .map(|e| e.with_path_prefix(path))
                .collect()
        })
    }
}

/// Blanket implementation for Option<T> where T: Validate
impl<T: Validate> Validate for Option<T> {
    fn validate(&self) -> Validation<(), Vec<ConfigError>> {
        match self {
            Some(value) => value.validate(),
            None => Validation::success(()),
        }
    }
}

/// Blanket implementation for Vec<T> where T: Validate
impl<T: Validate> Validate for Vec<T> {
    fn validate(&self) -> Validation<(), Vec<ConfigError>> {
        let results: Vec<_> = self.iter()
            .enumerate()
            .map(|(i, item)| item.validate_at(&format!("[{}]", i)))
            .collect();

        Validation::all(results).map(|_| ())
    }
}
```

### Validator Types

```rust
/// A validator function that checks a value
pub trait Validator<T: ?Sized> {
    fn validate(&self, value: &T, path: &str) -> Validation<(), Vec<ConfigError>>;
}

/// Built-in validators
pub mod validators {
    use super::*;

    // String validators
    pub struct NonEmpty;
    pub struct MinLength(pub usize);
    pub struct MaxLength(pub usize);
    pub struct Length(pub std::ops::RangeInclusive<usize>);
    pub struct Pattern(pub regex::Regex);
    pub struct Email;
    pub struct Url;

    // Numeric validators
    pub struct Range<T>(pub std::ops::RangeInclusive<T>);
    pub struct Positive;
    pub struct Negative;
    pub struct NonZero;

    // Collection validators
    pub struct Each<V>(pub V);

    // Path validators
    pub struct FileExists;
    pub struct DirExists;
    pub struct ParentExists;
    pub struct Extension(pub String);
}
```

### Validator Implementations

```rust
impl Validator<str> for NonEmpty {
    fn validate(&self, value: &str, path: &str) -> Validation<(), Vec<ConfigError>> {
        if value.is_empty() {
            Validation::fail(vec![ConfigError::ValidationError {
                path: path.to_string(),
                source_location: None,
                value: Some(String::new()),
                message: "value cannot be empty".to_string(),
            }])
        } else {
            Validation::success(())
        }
    }
}

impl Validator<str> for MinLength {
    fn validate(&self, value: &str, path: &str) -> Validation<(), Vec<ConfigError>> {
        if value.len() < self.0 {
            Validation::fail(vec![ConfigError::ValidationError {
                path: path.to_string(),
                source_location: None,
                value: Some(value.to_string()),
                message: format!("length {} is less than minimum {}", value.len(), self.0),
            }])
        } else {
            Validation::success(())
        }
    }
}

impl<T: PartialOrd + std::fmt::Display + Clone> Validator<T> for Range<T> {
    fn validate(&self, value: &T, path: &str) -> Validation<(), Vec<ConfigError>> {
        if !self.0.contains(value) {
            Validation::fail(vec![ConfigError::ValidationError {
                path: path.to_string(),
                source_location: None,
                value: Some(value.to_string()),
                message: format!(
                    "value {} is not in range {}..={}",
                    value, self.0.start(), self.0.end()
                ),
            }])
        } else {
            Validation::success(())
        }
    }
}

impl Validator<std::path::Path> for FileExists {
    fn validate(&self, value: &std::path::Path, path: &str) -> Validation<(), Vec<ConfigError>> {
        if !value.is_file() {
            Validation::fail(vec![ConfigError::ValidationError {
                path: path.to_string(),
                source_location: None,
                value: Some(value.display().to_string()),
                message: "file does not exist".to_string(),
            }])
        } else {
            Validation::success(())
        }
    }
}
```

### Field Validation Helper

```rust
/// Validate a field against multiple validators
pub fn validate_field<T, V>(
    value: &T,
    path: &str,
    validators: &[&dyn Validator<T>],
) -> Validation<(), Vec<ConfigError>>
where
    T: ?Sized,
{
    let results: Vec<_> = validators
        .iter()
        .map(|v| v.validate(value, path))
        .collect();

    Validation::all(results).map(|_| ())
}

/// Validate a nested struct
pub fn validate_nested<T: Validate>(
    value: &T,
    path: &str,
) -> Validation<(), Vec<ConfigError>> {
    value.validate_at(path)
}
```

### Custom Validator Support

```rust
/// Create a custom validator from a function
pub fn custom<T, F>(f: F) -> impl Validator<T>
where
    F: Fn(&T, &str) -> Validation<(), Vec<ConfigError>>,
{
    struct Custom<F>(F);

    impl<T, F> Validator<T> for Custom<F>
    where
        F: Fn(&T, &str) -> Validation<(), Vec<ConfigError>>,
    {
        fn validate(&self, value: &T, path: &str) -> Validation<(), Vec<ConfigError>> {
            (self.0)(value, path)
        }
    }

    Custom(f)
}
```

### Conditional Validation

```rust
/// Validator that only runs when a condition is true
pub struct When<V, F> {
    validator: V,
    condition: F,
}

impl<V, F, T> Validator<T> for When<V, F>
where
    V: Validator<T>,
    F: Fn() -> bool,
{
    fn validate(&self, value: &T, path: &str) -> Validation<(), Vec<ConfigError>> {
        if (self.condition)() {
            self.validator.validate(value, path)
        } else {
            Validation::success(())
        }
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
