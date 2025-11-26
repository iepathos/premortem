//! Validation trait and validators for configuration types.
//!
//! This module provides:
//! - The `Validate` trait that configuration types implement
//! - The `Validator` trait for composable validation functions
//! - Built-in validators for common patterns (strings, numbers, paths, collections)
//! - `ValidationContext` for source location lookup during validation
//!
//! # Stillwater Integration
//!
//! All validators integrate with stillwater's `Validation` type for error accumulation.
//! Use `Validation::all()` to combine multiple validators and collect ALL errors.
//!
//! # Example
//!
//! ```ignore
//! use premortem::{Validate, ConfigValidation, ConfigError, ConfigErrors};
//! use premortem::validate::{validate_field, validators::*};
//! use stillwater::Validation;
//!
//! struct ServerConfig {
//!     host: String,
//!     port: u16,
//! }
//!
//! impl Validate for ServerConfig {
//!     fn validate(&self) -> ConfigValidation<()> {
//!         Validation::all((
//!             validate_field(&self.host, "host", &[&NonEmpty]),
//!             validate_field(&self.port, "port", &[&Range(1..=65535)]),
//!         ))
//!         .map(|_| ())
//!     }
//! }
//! ```

use std::cell::RefCell;
use std::collections::HashMap;

use stillwater::Validation;

use crate::error::{ConfigError, ConfigErrors, ConfigValidation, SourceLocation};

// ============================================================================
// Validation Context (for source location lookup)
// ============================================================================

/// Map from config path to source location.
pub type SourceLocationMap = HashMap<String, SourceLocation>;

/// Context for validation with source location lookup.
///
/// This context is populated from `ConfigValues` during config building and
/// made available to validation code via thread-local storage. This enables
/// validation errors to include accurate source locations without changing
/// the `Validate` trait signature.
#[derive(Debug, Default)]
pub struct ValidationContext {
    locations: SourceLocationMap,
}

impl ValidationContext {
    /// Create a new validation context with the given source locations.
    pub fn new(locations: SourceLocationMap) -> Self {
        Self { locations }
    }

    /// Look up the source location for a config path.
    pub fn location_for(&self, path: &str) -> Option<&SourceLocation> {
        self.locations.get(path)
    }
}

// Thread-local storage for validation context.
// This is pragmatic (per stillwater philosophy) - it avoids breaking the Validate trait API
// while still enabling source location lookup from generated validation code.
thread_local! {
    static VALIDATION_CONTEXT: RefCell<Option<ValidationContext>> = const { RefCell::new(None) };
    static PATH_PREFIX: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Run a function with a validation context set.
///
/// The context is set for the duration of the function and cleared afterward.
/// This is used by `ConfigBuilder` to provide source locations during validation.
pub fn with_validation_context<F, R>(ctx: ValidationContext, f: F) -> R
where
    F: FnOnce() -> R,
{
    VALIDATION_CONTEXT.with(|cell| {
        *cell.borrow_mut() = Some(ctx);
    });
    let result = f();
    VALIDATION_CONTEXT.with(|cell| {
        *cell.borrow_mut() = None;
    });
    result
}

/// Look up the source location for a config path from the current context.
///
/// Returns `None` if no context is set or if the path is not found.
/// This is used by generated validation code to attach source locations to errors.
///
/// The lookup path is computed by prepending any active path prefixes (from nested
/// validation) to the given field path.
pub fn current_source_location(path: &str) -> Option<SourceLocation> {
    // Build full path by prepending the current prefix
    let full_path = PATH_PREFIX.with(|cell| {
        let prefixes = cell.borrow();
        if prefixes.is_empty() {
            path.to_string()
        } else {
            format!("{}.{}", prefixes.join("."), path)
        }
    });

    VALIDATION_CONTEXT.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|ctx| ctx.location_for(&full_path).cloned())
    })
}

/// Push a path prefix for nested validation.
///
/// Used by `validate_at` to track the current path context during nested struct validation.
pub fn push_path_prefix(prefix: &str) {
    PATH_PREFIX.with(|cell| {
        cell.borrow_mut().push(prefix.to_string());
    });
}

/// Pop a path prefix after nested validation completes.
pub fn pop_path_prefix() {
    PATH_PREFIX.with(|cell| {
        cell.borrow_mut().pop();
    });
}

/// Trait for validating configuration values.
///
/// Types implementing this trait can perform custom validation logic
/// after deserialization. The validation uses stillwater's `Validation`
/// type to accumulate all errors.
///
/// # Pure Core
///
/// Validators should be pure functions - no I/O allowed in the core validation.
/// Path validators like `FileExists` perform I/O but are acceptable for
/// configuration validation at startup time.
pub trait Validate {
    /// Validate this configuration value.
    ///
    /// Returns `ConfigValidation<()>` - either `Success(())` if validation
    /// passes, or `Failure(ConfigErrors)` with all accumulated validation errors.
    fn validate(&self) -> ConfigValidation<()>;

    /// Validate with a path prefix for error context.
    ///
    /// This adds context to all errors, following stillwater's error trail pattern.
    /// Used for nested struct validation.
    ///
    /// The path prefix is also pushed to thread-local storage so that source location
    /// lookups during nested validation use the correct full path (e.g., "server.host"
    /// instead of just "host").
    ///
    /// # Example
    ///
    /// ```ignore
    /// // If inner validation produces error at path "host",
    /// // validate_at("database") will produce error at path "database.host"
    /// database_config.validate_at("database")
    /// ```
    fn validate_at(&self, path: &str) -> ConfigValidation<()> {
        // Push prefix for source location lookups during nested validation
        push_path_prefix(path);
        let result = self.validate();
        pop_path_prefix();

        result.map_err(|errors| errors.with_path_prefix(path))
    }
}

/// Blanket implementation for types that don't need validation.
///
/// Any type that doesn't implement `Validate` will automatically pass validation.
/// This is implemented for the unit type as a no-op validator.
impl Validate for () {
    fn validate(&self) -> ConfigValidation<()> {
        Validation::Success(())
    }
}

/// Implementation for `Option<T>` where T: Validate.
///
/// None values pass validation; Some values delegate to the inner type.
impl<T: Validate> Validate for Option<T> {
    fn validate(&self) -> ConfigValidation<()> {
        match self {
            Some(inner) => inner.validate(),
            None => Validation::Success(()),
        }
    }
}

/// Implementation for `Vec<T>` where T: Validate.
///
/// Validates all elements and accumulates errors using stillwater's traverse pattern.
impl<T: Validate> Validate for Vec<T> {
    fn validate(&self) -> ConfigValidation<()> {
        if self.is_empty() {
            return Validation::Success(());
        }

        // Use traverse pattern to validate each item with index context
        let validations: Vec<ConfigValidation<()>> = self
            .iter()
            .enumerate()
            .map(|(i, item)| item.validate_at(&format!("[{}]", i)))
            .collect();
        Validation::all_vec(validations).map(|_| ())
    }
}

// Primitive types don't need validation
macro_rules! impl_validate_noop {
    ($($t:ty),*) => {
        $(
            impl Validate for $t {
                fn validate(&self) -> ConfigValidation<()> {
                    Validation::Success(())
                }
            }
        )*
    };
}

impl_validate_noop!(
    bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, char, String
);

impl Validate for &str {
    fn validate(&self) -> ConfigValidation<()> {
        Validation::Success(())
    }
}

impl Validate for std::path::PathBuf {
    fn validate(&self) -> ConfigValidation<()> {
        Validation::Success(())
    }
}

// ============================================================================
// Validator Trait
// ============================================================================

/// A validator function that checks a value and returns validation result.
///
/// Validators are pure functions: `&T -> ConfigValidation<()>`
/// They return either Success(()) or Failure(ConfigErrors).
///
/// # Type Parameters
///
/// - `T`: The type being validated (can be unsized, e.g., `str`, `[T]`)
pub trait Validator<T: ?Sized> {
    /// Validate a value at the given path.
    ///
    /// Returns `Success(())` if valid, or `Failure(ConfigErrors)` with an error
    /// that includes the path for context.
    fn validate(&self, value: &T, path: &str) -> ConfigValidation<()>;
}

/// Helper to create a validation failure with a single error.
fn fail(error: ConfigError) -> ConfigValidation<()> {
    Validation::Failure(ConfigErrors::single(error))
}

// ============================================================================
// Validation Helper Functions
// ============================================================================

/// Validate a field against multiple validators using Validation::all().
///
/// This is the core composition pattern - run all validators and accumulate errors.
/// Follows stillwater's "fail completely" philosophy.
///
/// # Example
///
/// ```ignore
/// use premortem::validate::{validate_field, validators::*};
///
/// let result = validate_field(&host, "host", &[&NonEmpty, &MinLength(3)]);
/// ```
pub fn validate_field<T>(
    value: &T,
    path: &str,
    validators: &[&dyn Validator<T>],
) -> ConfigValidation<()>
where
    T: ?Sized,
{
    if validators.is_empty() {
        return Validation::Success(());
    }

    let results: Vec<ConfigValidation<()>> =
        validators.iter().map(|v| v.validate(value, path)).collect();

    Validation::all_vec(results).map(|_| ())
}

/// Validate a nested struct with path context.
///
/// Delegates to the inner type's `validate_at` method.
pub fn validate_nested<T: Validate>(value: &T, path: &str) -> ConfigValidation<()> {
    value.validate_at(path)
}

/// Validate an optional nested struct.
///
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

// ============================================================================
// Built-in Validators
// ============================================================================

/// Built-in validators for common validation patterns.
///
/// All validators are zero-cost structs that implement the `Validator` trait.
/// They can be composed using `validate_field()` for multiple checks on one field.
pub mod validators {
    use super::*;
    use std::fmt::Display;
    use std::ops::RangeInclusive;
    use std::path::Path;

    // ========================================================================
    // String Validators
    // ========================================================================

    /// Validates that a string is not empty.
    #[derive(Debug, Clone, Copy)]
    pub struct NonEmpty;

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

    impl Validator<String> for NonEmpty {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <NonEmpty as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    /// Validates that a string has at least the specified length.
    #[derive(Debug, Clone, Copy)]
    pub struct MinLength(pub usize);

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

    impl Validator<String> for MinLength {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <MinLength as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    /// Validates that a string has at most the specified length.
    #[derive(Debug, Clone, Copy)]
    pub struct MaxLength(pub usize);

    impl Validator<str> for MaxLength {
        fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
            if value.len() > self.0 {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.to_string()),
                    message: format!("length {} exceeds maximum {}", value.len(), self.0),
                })
            } else {
                Validation::Success(())
            }
        }
    }

    impl Validator<String> for MaxLength {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <MaxLength as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    /// Validates that a string length is within a range (inclusive).
    #[derive(Debug, Clone)]
    pub struct Length(pub RangeInclusive<usize>);

    impl Validator<str> for Length {
        fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
            let len = value.len();
            if !self.0.contains(&len) {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.to_string()),
                    message: format!(
                        "length {} is not in range {}..={}",
                        len,
                        self.0.start(),
                        self.0.end()
                    ),
                })
            } else {
                Validation::Success(())
            }
        }
    }

    impl Validator<String> for Length {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <Length as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    /// Validates that a string matches a regular expression pattern.
    ///
    /// The pattern string is compiled on first use. For performance-critical
    /// code, consider pre-compiling with `regex::Regex`.
    #[derive(Debug, Clone)]
    pub struct Pattern(pub String);

    impl Pattern {
        /// Create a new pattern validator.
        pub fn new(pattern: impl Into<String>) -> Self {
            Self(pattern.into())
        }
    }

    impl Validator<str> for Pattern {
        fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
            // Compile regex - in production, consider caching
            match regex::Regex::new(&self.0) {
                Ok(re) => {
                    if re.is_match(value) {
                        Validation::Success(())
                    } else {
                        fail(ConfigError::ValidationError {
                            path: path.to_string(),
                            source_location: None,
                            value: Some(value.to_string()),
                            message: format!("value does not match pattern '{}'", self.0),
                        })
                    }
                }
                Err(e) => fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.to_string()),
                    message: format!("invalid pattern '{}': {}", self.0, e),
                }),
            }
        }
    }

    impl Validator<String> for Pattern {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <Pattern as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    /// Validates that a string is a valid email address.
    ///
    /// Uses a simplified RFC 5322-like pattern. For strict compliance,
    /// consider using the `email_address` crate.
    #[derive(Debug, Clone, Copy)]
    pub struct Email;

    impl Validator<str> for Email {
        fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
            // Simple email validation pattern
            // For strict RFC 5322, use email_address crate
            let email_pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$";
            let re = regex::Regex::new(email_pattern).expect("valid email regex");

            if re.is_match(value) {
                Validation::Success(())
            } else {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.to_string()),
                    message: "value is not a valid email address".to_string(),
                })
            }
        }
    }

    impl Validator<String> for Email {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <Email as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    /// Validates that a string is a valid URL.
    #[derive(Debug, Clone, Copy)]
    pub struct Url;

    impl Validator<str> for Url {
        fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
            // Simple URL pattern - for strict validation, use the url crate
            let url_pattern = r"^(https?|ftp)://[^\s/$.?#].[^\s]*$";
            let re = regex::Regex::new(url_pattern).expect("valid url regex");

            if re.is_match(value) {
                Validation::Success(())
            } else {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.to_string()),
                    message: "value is not a valid URL".to_string(),
                })
            }
        }
    }

    impl Validator<String> for Url {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <Url as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    // ========================================================================
    // Numeric Validators
    // ========================================================================

    /// Validates that a numeric value is within a range (inclusive).
    #[derive(Debug, Clone)]
    pub struct Range<T>(pub RangeInclusive<T>);

    impl<T> Validator<T> for Range<T>
    where
        T: PartialOrd + Display + Clone,
    {
        fn validate(&self, value: &T, path: &str) -> ConfigValidation<()> {
            if !self.0.contains(value) {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.to_string()),
                    message: format!(
                        "value {} is not in range {}..={}",
                        value,
                        self.0.start(),
                        self.0.end()
                    ),
                })
            } else {
                Validation::Success(())
            }
        }
    }

    /// Validates that a numeric value is positive (> 0).
    #[derive(Debug, Clone, Copy)]
    pub struct Positive;

    macro_rules! impl_positive_for_signed {
        ($($t:ty),*) => {
            $(
                impl Validator<$t> for Positive {
                    fn validate(&self, value: &$t, path: &str) -> ConfigValidation<()> {
                        if *value > 0 {
                            Validation::Success(())
                        } else {
                            fail(ConfigError::ValidationError {
                                path: path.to_string(),
                                source_location: None,
                                value: Some(value.to_string()),
                                message: "value must be positive".to_string(),
                            })
                        }
                    }
                }
            )*
        };
    }

    macro_rules! impl_positive_for_unsigned {
        ($($t:ty),*) => {
            $(
                impl Validator<$t> for Positive {
                    fn validate(&self, value: &$t, path: &str) -> ConfigValidation<()> {
                        if *value > 0 {
                            Validation::Success(())
                        } else {
                            fail(ConfigError::ValidationError {
                                path: path.to_string(),
                                source_location: None,
                                value: Some(value.to_string()),
                                message: "value must be positive".to_string(),
                            })
                        }
                    }
                }
            )*
        };
    }

    macro_rules! impl_positive_for_float {
        ($($t:ty),*) => {
            $(
                impl Validator<$t> for Positive {
                    fn validate(&self, value: &$t, path: &str) -> ConfigValidation<()> {
                        if *value > 0.0 {
                            Validation::Success(())
                        } else {
                            fail(ConfigError::ValidationError {
                                path: path.to_string(),
                                source_location: None,
                                value: Some(value.to_string()),
                                message: "value must be positive".to_string(),
                            })
                        }
                    }
                }
            )*
        };
    }

    impl_positive_for_signed!(i8, i16, i32, i64, i128, isize);
    impl_positive_for_unsigned!(u8, u16, u32, u64, u128, usize);
    impl_positive_for_float!(f32, f64);

    /// Validates that a numeric value is negative (< 0).
    #[derive(Debug, Clone, Copy)]
    pub struct Negative;

    macro_rules! impl_negative_for_signed {
        ($($t:ty),*) => {
            $(
                impl Validator<$t> for Negative {
                    fn validate(&self, value: &$t, path: &str) -> ConfigValidation<()> {
                        if *value < 0 {
                            Validation::Success(())
                        } else {
                            fail(ConfigError::ValidationError {
                                path: path.to_string(),
                                source_location: None,
                                value: Some(value.to_string()),
                                message: "value must be negative".to_string(),
                            })
                        }
                    }
                }
            )*
        };
    }

    macro_rules! impl_negative_for_float {
        ($($t:ty),*) => {
            $(
                impl Validator<$t> for Negative {
                    fn validate(&self, value: &$t, path: &str) -> ConfigValidation<()> {
                        if *value < 0.0 {
                            Validation::Success(())
                        } else {
                            fail(ConfigError::ValidationError {
                                path: path.to_string(),
                                source_location: None,
                                value: Some(value.to_string()),
                                message: "value must be negative".to_string(),
                            })
                        }
                    }
                }
            )*
        };
    }

    impl_negative_for_signed!(i8, i16, i32, i64, i128, isize);
    impl_negative_for_float!(f32, f64);

    /// Validates that a numeric value is non-zero.
    #[derive(Debug, Clone, Copy)]
    pub struct NonZero;

    macro_rules! impl_nonzero_for_int {
        ($($t:ty),*) => {
            $(
                impl Validator<$t> for NonZero {
                    fn validate(&self, value: &$t, path: &str) -> ConfigValidation<()> {
                        if *value != 0 {
                            Validation::Success(())
                        } else {
                            fail(ConfigError::ValidationError {
                                path: path.to_string(),
                                source_location: None,
                                value: Some(value.to_string()),
                                message: "value cannot be zero".to_string(),
                            })
                        }
                    }
                }
            )*
        };
    }

    macro_rules! impl_nonzero_for_float {
        ($($t:ty),*) => {
            $(
                impl Validator<$t> for NonZero {
                    fn validate(&self, value: &$t, path: &str) -> ConfigValidation<()> {
                        #[allow(clippy::float_cmp)]
                        if *value != 0.0 {
                            Validation::Success(())
                        } else {
                            fail(ConfigError::ValidationError {
                                path: path.to_string(),
                                source_location: None,
                                value: Some(value.to_string()),
                                message: "value cannot be zero".to_string(),
                            })
                        }
                    }
                }
            )*
        };
    }

    impl_nonzero_for_int!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
    impl_nonzero_for_float!(f32, f64);

    // ========================================================================
    // Collection Validators
    // ========================================================================

    /// Validates that a collection is not empty.
    #[derive(Debug, Clone, Copy)]
    pub struct NonEmptyCollection;

    impl<T> Validator<Vec<T>> for NonEmptyCollection {
        fn validate(&self, value: &Vec<T>, path: &str) -> ConfigValidation<()> {
            if value.is_empty() {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some("[]".to_string()),
                    message: "collection cannot be empty".to_string(),
                })
            } else {
                Validation::Success(())
            }
        }
    }

    impl<T> Validator<[T]> for NonEmptyCollection {
        fn validate(&self, value: &[T], path: &str) -> ConfigValidation<()> {
            if value.is_empty() {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some("[]".to_string()),
                    message: "collection cannot be empty".to_string(),
                })
            } else {
                Validation::Success(())
            }
        }
    }

    /// Validates that a collection has at least the specified number of elements.
    #[derive(Debug, Clone, Copy)]
    pub struct MinItems(pub usize);

    impl<T> Validator<Vec<T>> for MinItems {
        fn validate(&self, value: &Vec<T>, path: &str) -> ConfigValidation<()> {
            if value.len() < self.0 {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(format!("[{} items]", value.len())),
                    message: format!(
                        "collection has {} items, minimum is {}",
                        value.len(),
                        self.0
                    ),
                })
            } else {
                Validation::Success(())
            }
        }
    }

    impl<T> Validator<[T]> for MinItems {
        fn validate(&self, value: &[T], path: &str) -> ConfigValidation<()> {
            if value.len() < self.0 {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(format!("[{} items]", value.len())),
                    message: format!(
                        "collection has {} items, minimum is {}",
                        value.len(),
                        self.0
                    ),
                })
            } else {
                Validation::Success(())
            }
        }
    }

    /// Validates that a collection has at most the specified number of elements.
    #[derive(Debug, Clone, Copy)]
    pub struct MaxItems(pub usize);

    impl<T> Validator<Vec<T>> for MaxItems {
        fn validate(&self, value: &Vec<T>, path: &str) -> ConfigValidation<()> {
            if value.len() > self.0 {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(format!("[{} items]", value.len())),
                    message: format!(
                        "collection has {} items, maximum is {}",
                        value.len(),
                        self.0
                    ),
                })
            } else {
                Validation::Success(())
            }
        }
    }

    impl<T> Validator<[T]> for MaxItems {
        fn validate(&self, value: &[T], path: &str) -> ConfigValidation<()> {
            if value.len() > self.0 {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(format!("[{} items]", value.len())),
                    message: format!(
                        "collection has {} items, maximum is {}",
                        value.len(),
                        self.0
                    ),
                })
            } else {
                Validation::Success(())
            }
        }
    }

    /// Validates each item in a collection using the given validator.
    ///
    /// Uses stillwater's traverse pattern to accumulate ALL errors across ALL items.
    #[derive(Debug, Clone)]
    pub struct Each<V>(pub V);

    impl<V, T> Validator<Vec<T>> for Each<V>
    where
        V: Validator<T>,
    {
        fn validate(&self, value: &Vec<T>, path: &str) -> ConfigValidation<()> {
            if value.is_empty() {
                return Validation::Success(());
            }

            let results: Vec<ConfigValidation<()>> = value
                .iter()
                .enumerate()
                .map(|(i, item)| self.0.validate(item, &format!("{}[{}]", path, i)))
                .collect();

            Validation::all_vec(results).map(|_| ())
        }
    }

    impl<V, T> Validator<[T]> for Each<V>
    where
        V: Validator<T>,
    {
        fn validate(&self, value: &[T], path: &str) -> ConfigValidation<()> {
            if value.is_empty() {
                return Validation::Success(());
            }

            let results: Vec<ConfigValidation<()>> = value
                .iter()
                .enumerate()
                .map(|(i, item)| self.0.validate(item, &format!("{}[{}]", path, i)))
                .collect();

            Validation::all_vec(results).map(|_| ())
        }
    }

    // ========================================================================
    // Path Validators
    // ========================================================================

    /// Validates that a path points to an existing file.
    ///
    /// **Note**: This performs I/O (filesystem check). For strict pure core /
    /// imperative shell separation, consider using stillwater's `Effect` type.
    /// Kept here for convenience in config validation at startup.
    #[derive(Debug, Clone, Copy)]
    pub struct FileExists;

    impl Validator<Path> for FileExists {
        fn validate(&self, value: &Path, path: &str) -> ConfigValidation<()> {
            if value.is_file() {
                Validation::Success(())
            } else {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.display().to_string()),
                    message: "file does not exist".to_string(),
                })
            }
        }
    }

    impl Validator<std::path::PathBuf> for FileExists {
        fn validate(&self, value: &std::path::PathBuf, path: &str) -> ConfigValidation<()> {
            <FileExists as Validator<Path>>::validate(self, value.as_path(), path)
        }
    }

    impl Validator<str> for FileExists {
        fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
            <FileExists as Validator<Path>>::validate(self, Path::new(value), path)
        }
    }

    impl Validator<String> for FileExists {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <FileExists as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    /// Validates that a path points to an existing directory.
    ///
    /// **Note**: This performs I/O (filesystem check).
    #[derive(Debug, Clone, Copy)]
    pub struct DirExists;

    impl Validator<Path> for DirExists {
        fn validate(&self, value: &Path, path: &str) -> ConfigValidation<()> {
            if value.is_dir() {
                Validation::Success(())
            } else {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.display().to_string()),
                    message: "directory does not exist".to_string(),
                })
            }
        }
    }

    impl Validator<std::path::PathBuf> for DirExists {
        fn validate(&self, value: &std::path::PathBuf, path: &str) -> ConfigValidation<()> {
            <DirExists as Validator<Path>>::validate(self, value.as_path(), path)
        }
    }

    impl Validator<str> for DirExists {
        fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
            <DirExists as Validator<Path>>::validate(self, Path::new(value), path)
        }
    }

    impl Validator<String> for DirExists {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <DirExists as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    /// Validates that a path's parent directory exists.
    ///
    /// Useful for validating output file paths before writing.
    ///
    /// **Note**: This performs I/O (filesystem check).
    #[derive(Debug, Clone, Copy)]
    pub struct ParentExists;

    impl Validator<Path> for ParentExists {
        fn validate(&self, value: &Path, path: &str) -> ConfigValidation<()> {
            match value.parent() {
                Some(parent) if parent.is_dir() || parent.as_os_str().is_empty() => {
                    Validation::Success(())
                }
                Some(_) => fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.display().to_string()),
                    message: "parent directory does not exist".to_string(),
                }),
                None => fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.display().to_string()),
                    message: "path has no parent directory".to_string(),
                }),
            }
        }
    }

    impl Validator<std::path::PathBuf> for ParentExists {
        fn validate(&self, value: &std::path::PathBuf, path: &str) -> ConfigValidation<()> {
            <ParentExists as Validator<Path>>::validate(self, value.as_path(), path)
        }
    }

    impl Validator<str> for ParentExists {
        fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
            <ParentExists as Validator<Path>>::validate(self, Path::new(value), path)
        }
    }

    impl Validator<String> for ParentExists {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <ParentExists as Validator<str>>::validate(self, value.as_str(), path)
        }
    }

    /// Validates that a path has the specified file extension.
    #[derive(Debug, Clone)]
    pub struct Extension(pub String);

    impl Extension {
        /// Create a new extension validator.
        pub fn new(ext: impl Into<String>) -> Self {
            Self(ext.into())
        }
    }

    impl Validator<Path> for Extension {
        fn validate(&self, value: &Path, path: &str) -> ConfigValidation<()> {
            match value.extension() {
                Some(ext) if ext == self.0.as_str() => Validation::Success(()),
                Some(ext) => fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.display().to_string()),
                    message: format!(
                        "expected extension '{}', found '{}'",
                        self.0,
                        ext.to_string_lossy()
                    ),
                }),
                None => fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.display().to_string()),
                    message: format!("expected extension '{}', found none", self.0),
                }),
            }
        }
    }

    impl Validator<std::path::PathBuf> for Extension {
        fn validate(&self, value: &std::path::PathBuf, path: &str) -> ConfigValidation<()> {
            <Extension as Validator<Path>>::validate(self, value.as_path(), path)
        }
    }

    impl Validator<str> for Extension {
        fn validate(&self, value: &str, path: &str) -> ConfigValidation<()> {
            <Extension as Validator<Path>>::validate(self, Path::new(value), path)
        }
    }

    impl Validator<String> for Extension {
        fn validate(&self, value: &String, path: &str) -> ConfigValidation<()> {
            <Extension as Validator<str>>::validate(self, value.as_str(), path)
        }
    }
}

// ============================================================================
// Custom Validator Support
// ============================================================================

/// Create a custom validator from a pure function.
///
/// # Example
///
/// ```ignore
/// use premortem::validate::custom;
/// use premortem::{ConfigError, ConfigValidation};
/// use stillwater::Validation;
///
/// let even_validator = custom(|value: &i32, path: &str| {
///     if value % 2 == 0 {
///         Validation::Success(())
///     } else {
///         Validation::Failure(ConfigErrors::single(ConfigError::ValidationError {
///             path: path.to_string(),
///             source_location: None,
///             value: Some(value.to_string()),
///             message: "value must be even".to_string(),
///         }))
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

// ============================================================================
// Conditional Validation
// ============================================================================

/// Validator that only runs when a condition is true.
///
/// Useful for "when X is set, Y must be valid" patterns.
///
/// # Example
///
/// ```ignore
/// use premortem::validate::{When, validators::NonEmpty};
///
/// // Only validate host is non-empty when use_remote is true
/// let conditional = When::new(NonEmpty, || config.use_remote);
/// ```
pub struct When<V, F> {
    validator: V,
    condition: F,
}

impl<V, F> When<V, F> {
    /// Create a new conditional validator.
    pub fn new(validator: V, condition: F) -> Self {
        Self {
            validator,
            condition,
        }
    }
}

impl<V, F, T> Validator<T> for When<V, F>
where
    V: Validator<T>,
    F: Fn() -> bool,
    T: ?Sized,
{
    fn validate(&self, value: &T, path: &str) -> ConfigValidation<()> {
        if (self.condition)() {
            self.validator.validate(value, path)
        } else {
            Validation::Success(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validators::*;
    use super::*;
    use std::path::{Path, PathBuf};

    // ========================================================================
    // Validate Trait Tests
    // ========================================================================

    #[test]
    fn test_unit_validate() {
        let result = ().validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_option_validate_none() {
        let opt: Option<String> = None;
        let result = opt.validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_option_validate_some() {
        let opt = Some("value".to_string());
        let result = opt.validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_vec_validate_empty() {
        let v: Vec<String> = vec![];
        let result = v.validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_vec_validate_with_items() {
        let v = vec!["a".to_string(), "b".to_string()];
        let result = v.validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_primitive_validate() {
        assert!(42i32.validate().is_success());
        assert!((std::f64::consts::E).validate().is_success());
        assert!(true.validate().is_success());
        assert!("hello".to_string().validate().is_success());
    }

    #[test]
    fn test_pathbuf_validate() {
        let path = PathBuf::from("/some/path");
        assert!(path.validate().is_success());
    }

    // ========================================================================
    // validate_at Tests
    // ========================================================================

    struct FailingConfig;

    impl Validate for FailingConfig {
        fn validate(&self) -> ConfigValidation<()> {
            fail(ConfigError::ValidationError {
                path: "inner".to_string(),
                source_location: None,
                value: None,
                message: "always fails".to_string(),
            })
        }
    }

    #[test]
    fn test_validate_at_prefixes_path() {
        let config = FailingConfig;
        let result = config.validate_at("outer");
        assert!(result.is_failure());

        if let Validation::Failure(errors) = result {
            assert_eq!(errors.first().path(), Some("outer.inner"));
        }
    }

    // ========================================================================
    // String Validator Tests
    // ========================================================================

    #[test]
    fn test_non_empty_success() {
        let result = NonEmpty.validate("hello", "field");
        assert!(result.is_success());
    }

    #[test]
    fn test_non_empty_failure() {
        let result = NonEmpty.validate("", "field");
        assert!(result.is_failure());
    }

    #[test]
    fn test_min_length_success() {
        let result = MinLength(3).validate("hello", "field");
        assert!(result.is_success());
    }

    #[test]
    fn test_min_length_failure() {
        let result = MinLength(10).validate("hi", "field");
        assert!(result.is_failure());
    }

    #[test]
    fn test_max_length_success() {
        let result = MaxLength(10).validate("hello", "field");
        assert!(result.is_success());
    }

    #[test]
    fn test_max_length_failure() {
        let result = MaxLength(3).validate("hello", "field");
        assert!(result.is_failure());
    }

    #[test]
    fn test_length_success() {
        let result = Length(3..=10).validate("hello", "field");
        assert!(result.is_success());
    }

    #[test]
    fn test_length_failure_too_short() {
        let result = Length(5..=10).validate("hi", "field");
        assert!(result.is_failure());
    }

    #[test]
    fn test_length_failure_too_long() {
        let result = Length(1..=3).validate("hello", "field");
        assert!(result.is_failure());
    }

    #[test]
    fn test_pattern_success() {
        let result = Pattern::new(r"^\d+$").validate("12345", "field");
        assert!(result.is_success());
    }

    #[test]
    fn test_pattern_failure() {
        let result = Pattern::new(r"^\d+$").validate("abc", "field");
        assert!(result.is_failure());
    }

    #[test]
    fn test_email_success() {
        let result = Email.validate("user@example.com", "email");
        assert!(result.is_success());
    }

    #[test]
    fn test_email_failure() {
        let result = Email.validate("not-an-email", "email");
        assert!(result.is_failure());
    }

    #[test]
    fn test_url_success() {
        let result = Url.validate("https://example.com", "url");
        assert!(result.is_success());
    }

    #[test]
    fn test_url_failure() {
        let result = Url.validate("not-a-url", "url");
        assert!(result.is_failure());
    }

    // ========================================================================
    // Numeric Validator Tests
    // ========================================================================

    #[test]
    fn test_range_success() {
        let result = Range(1..=100).validate(&50i32, "field");
        assert!(result.is_success());
    }

    #[test]
    fn test_range_failure_below() {
        let result = Range(10..=100).validate(&5i32, "field");
        assert!(result.is_failure());
    }

    #[test]
    fn test_range_failure_above() {
        let result = Range(1..=10).validate(&50i32, "field");
        assert!(result.is_failure());
    }

    #[test]
    fn test_positive_success() {
        assert!(Positive.validate(&42i32, "field").is_success());
        assert!(Positive.validate(&1u32, "field").is_success());
        assert!(Positive.validate(&0.5f64, "field").is_success());
    }

    #[test]
    fn test_positive_failure() {
        assert!(Positive.validate(&-1i32, "field").is_failure());
        assert!(Positive.validate(&0i32, "field").is_failure());
        assert!(Positive.validate(&0u32, "field").is_failure());
        assert!(Positive.validate(&-0.5f64, "field").is_failure());
    }

    #[test]
    fn test_negative_success() {
        assert!(Negative.validate(&-42i32, "field").is_success());
        assert!(Negative.validate(&-0.5f64, "field").is_success());
    }

    #[test]
    fn test_negative_failure() {
        assert!(Negative.validate(&42i32, "field").is_failure());
        assert!(Negative.validate(&0i32, "field").is_failure());
        assert!(Negative.validate(&0.5f64, "field").is_failure());
    }

    #[test]
    fn test_non_zero_success() {
        assert!(NonZero.validate(&42i32, "field").is_success());
        assert!(NonZero.validate(&-1i32, "field").is_success());
        assert!(NonZero.validate(&1u32, "field").is_success());
        assert!(NonZero.validate(&0.5f64, "field").is_success());
    }

    #[test]
    fn test_non_zero_failure() {
        assert!(NonZero.validate(&0i32, "field").is_failure());
        assert!(NonZero.validate(&0u32, "field").is_failure());
        assert!(NonZero.validate(&0.0f64, "field").is_failure());
    }

    // ========================================================================
    // Collection Validator Tests
    // ========================================================================

    #[test]
    fn test_non_empty_collection_success() {
        let v = vec![1, 2, 3];
        let result = NonEmptyCollection.validate(&v, "items");
        assert!(result.is_success());
    }

    #[test]
    fn test_non_empty_collection_failure() {
        let v: Vec<i32> = vec![];
        let result = NonEmptyCollection.validate(&v, "items");
        assert!(result.is_failure());
    }

    #[test]
    fn test_min_items_success() {
        let v = vec![1, 2, 3];
        let result = MinItems(2).validate(&v, "items");
        assert!(result.is_success());
    }

    #[test]
    fn test_min_items_failure() {
        let v = vec![1];
        let result = MinItems(3).validate(&v, "items");
        assert!(result.is_failure());
    }

    #[test]
    fn test_max_items_success() {
        let v = vec![1, 2, 3];
        let result = MaxItems(5).validate(&v, "items");
        assert!(result.is_success());
    }

    #[test]
    fn test_max_items_failure() {
        let v = vec![1, 2, 3, 4, 5];
        let result = MaxItems(3).validate(&v, "items");
        assert!(result.is_failure());
    }

    #[test]
    fn test_each_success() {
        let v = vec![1, 2, 3];
        let result = Each(Positive).validate(&v, "items");
        assert!(result.is_success());
    }

    #[test]
    fn test_each_failure_accumulates_errors() {
        let v = vec![1, -2, -3, 4];
        let result = Each(Positive).validate(&v, "items");
        assert!(result.is_failure());

        if let Validation::Failure(errors) = result {
            // Should have 2 errors for -2 and -3
            assert_eq!(errors.len(), 2);
        }
    }

    #[test]
    fn test_each_empty_collection() {
        let v: Vec<i32> = vec![];
        let result = Each(Positive).validate(&v, "items");
        assert!(result.is_success());
    }

    // ========================================================================
    // Path Validator Tests
    // ========================================================================

    #[test]
    fn test_extension_success() {
        let result = Extension::new("toml").validate(Path::new("config.toml"), "file");
        assert!(result.is_success());
    }

    #[test]
    fn test_extension_failure_wrong() {
        let result = Extension::new("toml").validate(Path::new("config.json"), "file");
        assert!(result.is_failure());
    }

    #[test]
    fn test_extension_failure_none() {
        let result = Extension::new("toml").validate(Path::new("config"), "file");
        assert!(result.is_failure());
    }

    // ========================================================================
    // validate_field Tests
    // ========================================================================

    #[test]
    fn test_validate_field_empty_validators() {
        let empty: &[&dyn Validator<str>] = &[];
        let result = validate_field("value", "field", empty);
        assert!(result.is_success());
    }

    #[test]
    fn test_validate_field_single_validator() {
        let result = validate_field("hello", "field", &[&NonEmpty]);
        assert!(result.is_success());
    }

    #[test]
    fn test_validate_field_multiple_validators_all_pass() {
        let result = validate_field("hello", "field", &[&NonEmpty, &MinLength(3)]);
        assert!(result.is_success());
    }

    #[test]
    fn test_validate_field_accumulates_errors() {
        // Empty string fails both NonEmpty and MinLength
        let result = validate_field("", "field", &[&NonEmpty, &MinLength(3)]);
        assert!(result.is_failure());

        if let Validation::Failure(errors) = result {
            assert_eq!(errors.len(), 2);
        }
    }

    // ========================================================================
    // validate_nested Tests
    // ========================================================================

    struct InnerConfig {
        value: i32,
    }

    impl Validate for InnerConfig {
        fn validate(&self) -> ConfigValidation<()> {
            if self.value > 0 {
                Validation::Success(())
            } else {
                fail(ConfigError::ValidationError {
                    path: "value".to_string(),
                    source_location: None,
                    value: Some(self.value.to_string()),
                    message: "must be positive".to_string(),
                })
            }
        }
    }

    #[test]
    fn test_validate_nested_success() {
        let inner = InnerConfig { value: 42 };
        let result = validate_nested(&inner, "config");
        assert!(result.is_success());
    }

    #[test]
    fn test_validate_nested_failure() {
        let inner = InnerConfig { value: -1 };
        let result = validate_nested(&inner, "config");
        assert!(result.is_failure());

        if let Validation::Failure(errors) = result {
            assert_eq!(errors.first().path(), Some("config.value"));
        }
    }

    #[test]
    fn test_validate_optional_nested_none() {
        let opt: Option<InnerConfig> = None;
        let result = validate_optional_nested(&opt, "config");
        assert!(result.is_success());
    }

    #[test]
    fn test_validate_optional_nested_some() {
        let opt = Some(InnerConfig { value: -1 });
        let result = validate_optional_nested(&opt, "config");
        assert!(result.is_failure());
    }

    // ========================================================================
    // Custom Validator Tests
    // ========================================================================

    #[test]
    fn test_custom_validator() {
        let even_validator = custom(|value: &i32, path: &str| {
            if value % 2 == 0 {
                Validation::Success(())
            } else {
                fail(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.to_string()),
                    message: "value must be even".to_string(),
                })
            }
        });

        assert!(even_validator.validate(&4, "num").is_success());
        assert!(even_validator.validate(&3, "num").is_failure());
    }

    // ========================================================================
    // Conditional Validator Tests
    // ========================================================================

    #[test]
    fn test_when_condition_true() {
        let validator = When::new(NonEmpty, || true);
        let result = validator.validate("", "field");
        assert!(result.is_failure());
    }

    #[test]
    fn test_when_condition_false() {
        let validator = When::new(NonEmpty, || false);
        let result = validator.validate("", "field");
        assert!(result.is_success());
    }

    // ========================================================================
    // Integration Tests
    // ========================================================================

    struct DatabaseConfig {
        host: String,
        port: u16,
        pool_size: u32,
    }

    impl Validate for DatabaseConfig {
        fn validate(&self) -> ConfigValidation<()> {
            let validations = vec![
                validate_field(&self.host, "host", &[&NonEmpty]),
                validate_field(&self.port, "port", &[&Range(1..=65535)]),
                validate_field(&self.pool_size, "pool_size", &[&Range(1..=100)]),
            ];
            Validation::all_vec(validations).map(|_| ())
        }
    }

    #[test]
    fn test_database_config_valid() {
        let config = DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            pool_size: 10,
        };
        assert!(config.validate().is_success());
    }

    #[test]
    fn test_database_config_accumulates_all_errors() {
        let config = DatabaseConfig {
            host: "".to_string(),
            port: 0,
            pool_size: 200,
        };

        let result = config.validate();
        assert!(result.is_failure());

        if let Validation::Failure(errors) = result {
            assert_eq!(errors.len(), 3);
        }
    }

    // Test Vec validation with custom type
    #[test]
    fn test_vec_custom_validate() {
        let configs = vec![
            InnerConfig { value: 1 },
            InnerConfig { value: -1 },
            InnerConfig { value: -2 },
        ];

        let result = configs.validate();
        assert!(result.is_failure());

        // Should have accumulated 2 errors with indexed paths
        if let Validation::Failure(errors) = result {
            assert_eq!(errors.len(), 2);
            let paths: Vec<_> = errors.iter().filter_map(|e| e.path()).collect();
            assert!(paths.contains(&"[1].value"));
            assert!(paths.contains(&"[2].value"));
        }
    }

    // ========================================================================
    // ValidationContext Tests
    // ========================================================================

    #[test]
    fn test_validation_context_lookup() {
        let mut locations = SourceLocationMap::new();
        locations.insert(
            "host".to_string(),
            SourceLocation::new("config.toml").with_line(5),
        );
        locations.insert(
            "port".to_string(),
            SourceLocation::new("config.toml").with_line(6),
        );

        let ctx = ValidationContext::new(locations);

        let host_loc = ctx.location_for("host").unwrap();
        assert_eq!(host_loc.source, "config.toml");
        assert_eq!(host_loc.line, Some(5));

        let port_loc = ctx.location_for("port").unwrap();
        assert_eq!(port_loc.line, Some(6));

        // Non-existent path returns None
        assert!(ctx.location_for("missing").is_none());
    }

    #[test]
    fn test_with_validation_context() {
        let mut locations = SourceLocationMap::new();
        locations.insert(
            "test_field".to_string(),
            SourceLocation::new("test.toml").with_line(10),
        );

        let ctx = ValidationContext::new(locations);

        // Before context is set, returns None
        assert!(current_source_location("test_field").is_none());

        // Within context, returns location
        let result = with_validation_context(ctx, || {
            let loc = current_source_location("test_field");
            assert!(loc.is_some());
            let loc = loc.unwrap();
            assert_eq!(loc.source, "test.toml");
            assert_eq!(loc.line, Some(10));
            "success"
        });

        assert_eq!(result, "success");

        // After context is cleared, returns None again
        assert!(current_source_location("test_field").is_none());
    }

    #[test]
    fn test_context_clears_on_completion() {
        let mut locations = SourceLocationMap::new();
        locations.insert("field".to_string(), SourceLocation::new("a.toml"));

        let ctx = ValidationContext::new(locations);
        with_validation_context(ctx, || ());

        // Context should be cleared
        assert!(current_source_location("field").is_none());
    }

    #[test]
    fn test_path_prefix_for_nested_lookup() {
        let mut locations = SourceLocationMap::new();
        locations.insert(
            "server.host".to_string(),
            SourceLocation::new("config.toml").with_line(3),
        );
        locations.insert(
            "server.port".to_string(),
            SourceLocation::new("config.toml").with_line(4),
        );
        locations.insert(
            "database.host".to_string(),
            SourceLocation::new("config.toml").with_line(7),
        );

        let ctx = ValidationContext::new(locations);

        with_validation_context(ctx, || {
            // Without prefix, "host" doesn't find anything
            assert!(current_source_location("host").is_none());

            // With "server" prefix, "host" finds "server.host"
            push_path_prefix("server");
            let loc = current_source_location("host");
            assert!(loc.is_some());
            let loc = loc.unwrap();
            assert_eq!(loc.source, "config.toml");
            assert_eq!(loc.line, Some(3));

            // port also works with prefix
            let port_loc = current_source_location("port").unwrap();
            assert_eq!(port_loc.line, Some(4));

            pop_path_prefix();

            // After popping, "host" doesn't find anything again
            assert!(current_source_location("host").is_none());

            // With "database" prefix
            push_path_prefix("database");
            let db_loc = current_source_location("host").unwrap();
            assert_eq!(db_loc.line, Some(7));
            pop_path_prefix();
        });
    }

    #[test]
    fn test_nested_path_prefix_stacking() {
        let mut locations = SourceLocationMap::new();
        locations.insert(
            "outer.inner.field".to_string(),
            SourceLocation::new("config.toml").with_line(10),
        );

        let ctx = ValidationContext::new(locations);

        with_validation_context(ctx, || {
            // No prefix - not found
            assert!(current_source_location("field").is_none());

            // Single prefix - still not found
            push_path_prefix("outer");
            assert!(current_source_location("field").is_none());

            // Nested prefix - found
            push_path_prefix("inner");
            let loc = current_source_location("field");
            assert!(loc.is_some());
            assert_eq!(loc.unwrap().line, Some(10));

            // Pop inner prefix
            pop_path_prefix();
            assert!(current_source_location("field").is_none());

            // Pop outer prefix
            pop_path_prefix();
        });
    }
}
