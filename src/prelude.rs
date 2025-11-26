//! Convenient re-exports for common premortem usage.
//!
//! # Quick Start
//!
//! For most users, import the prelude:
//!
//! ```ignore
//! use premortem::prelude::*;
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize)]
//! struct AppConfig {
//!     host: String,
//!     port: u16,
//! }
//!
//! impl Validate for AppConfig {
//!     fn validate(&self) -> ConfigValidation<()> {
//!         if self.port > 0 {
//!             Validation::Success(())
//!         } else {
//!             Validation::fail_with(ConfigError::ValidationError {
//!                 path: "port".to_string(),
//!                 source_location: None,
//!                 value: Some(self.port.to_string()),
//!                 message: "port must be positive".to_string(),
//!             })
//!         }
//!     }
//! }
//!
//! fn main() -> Result<(), ConfigErrors> {
//!     let config = Config::<AppConfig>::builder()
//!         .source(Toml::file("config.toml"))
//!         .source(Env::new().prefix("APP"))
//!         .build()?;
//!
//!     println!("Running on {}:{}", config.host, config.port);
//!     Ok(())
//! }
//! ```
//!
//! # Import Patterns
//!
//! ## Quick Start (Recommended)
//!
//! ```ignore
//! use premortem::prelude::*;
//! ```
//!
//! ## Selective Imports
//!
//! Import only what you need:
//!
//! ```ignore
//! use premortem::{Config, Validate};
//! use premortem::error::ConfigErrors;
//! ```
//!
//! ## Advanced: Direct Stillwater Access
//!
//! For custom sources or advanced patterns:
//!
//! ```ignore
//! use premortem::prelude::*;
//! use stillwater::Effect;  // Direct stillwater access for custom sources
//! ```

// ============================================================================
// Stillwater re-exports (core functional programming types)
// ============================================================================

/// Result type with error accumulation. Use `Validation::all()` to combine
/// multiple validations and collect ALL errors.
///
/// # Example
///
/// ```ignore
/// use premortem::prelude::*;
///
/// let result = Validation::all((
///     validate_host(&config.host),
///     validate_port(config.port),
/// ));
/// ```
pub use stillwater::Validation;

/// Trait for combining values. `ConfigErrors` implements this for error accumulation.
///
/// When multiple validations fail, their errors are combined using `Semigroup::combine`.
pub use stillwater::Semigroup;

/// Guaranteed non-empty collection. Underlying type for `ConfigErrors`.
///
/// This ensures that error collections always have at least one error,
/// preventing "empty error list" bugs.
pub use stillwater::NonEmptyVec;

// ============================================================================
// Error types
// ============================================================================

/// Individual configuration error with source location.
pub use crate::error::ConfigError;

/// Non-empty collection of errors. Implements `Semigroup` for accumulation.
///
/// Use `ConfigErrors::single()` to create from one error, or
/// `ConfigErrors::from_vec()` for multiple.
pub use crate::error::ConfigErrors;

/// Type alias: `Validation<T, ConfigErrors>`. The standard result type.
///
/// All premortem APIs use this type for validation results.
pub use crate::error::ConfigValidation;

/// Extension trait for creating failing validations easily.
pub use crate::error::ConfigValidationExt;

/// Location where a configuration value originated.
///
/// Tracks source file, line, and column for precise error reporting.
pub use crate::error::SourceLocation;

/// Kinds of source loading errors.
pub use crate::error::SourceErrorKind;

/// Group errors by their source for organized reporting.
pub use crate::error::group_by_source;

// ============================================================================
// Core config types
// ============================================================================

/// The main configuration container wrapping validated config.
///
/// Use `Config::builder()` to construct configuration from sources.
pub use crate::config::Config;

/// Builder for constructing configuration from multiple sources.
pub use crate::config::ConfigBuilder;

// ============================================================================
// Sources
// ============================================================================

/// Trait for configuration sources. Implement for custom sources.
pub use crate::source::Source;

/// Intermediate representation of configuration values.
pub use crate::source::ConfigValues;

/// Pure function to merge multiple ConfigValues by priority.
pub use crate::source::merge_config_values;

/// JSON file configuration source (requires `json` feature).
#[cfg(feature = "json")]
pub use crate::sources::Json;

/// TOML file configuration source (requires `toml` feature).
#[cfg(feature = "toml")]
pub use crate::sources::Toml;

/// Environment variable configuration source.
pub use crate::sources::Env;

/// Default values configuration source.
pub use crate::sources::Defaults;

/// Partial defaults builder for specific paths.
pub use crate::sources::PartialDefaults;

// ============================================================================
// Environment abstractions
// ============================================================================

/// Trait for abstracting I/O operations. Enables testable configuration loading.
pub use crate::env::ConfigEnv;

/// Real environment implementation for production use.
pub use crate::env::RealEnv;

/// Mock environment for testing.
pub use crate::env::MockEnv;

// ============================================================================
// Validation
// ============================================================================

/// Trait for types that can be validated.
///
/// Implement this trait to add custom validation logic to your config types.
pub use crate::validate::Validate;

/// Trait for individual validators.
pub use crate::validate::Validator;

/// Validate a field against multiple validators.
pub use crate::validate::validate_field;

/// Validate a nested struct with path context.
pub use crate::validate::validate_nested;

/// Validate an optional nested struct.
pub use crate::validate::validate_optional_nested;

/// Create a custom validator from a pure function.
pub use crate::validate::custom;

/// Conditional validator that only runs when a condition is true.
pub use crate::validate::When;

// ============================================================================
// Built-in validators
// ============================================================================

/// Built-in validators for common validation patterns.
pub mod validators {
    pub use crate::validate::validators::{
        // Path validators
        DirExists,
        // Collection validators
        Each,
        // String validators
        Email,
        Extension,
        FileExists,
        Length,
        MaxItems,
        MaxLength,
        MinItems,
        MinLength,
        // Numeric validators
        Negative,
        NonEmpty,
        NonEmptyCollection,
        NonZero,
        ParentExists,
        Pattern,
        Positive,
        Range,
        Url,
    };
}

// ============================================================================
// Value types
// ============================================================================

/// Untyped configuration value.
pub use crate::value::Value;

/// Configuration value with source location tracking.
pub use crate::value::ConfigValue;

// ============================================================================
// Tracing (debugging configuration origin)
// ============================================================================

/// Configuration with tracing information.
pub use crate::trace::TracedConfig;

/// A value with its source information.
pub use crate::trace::TracedValue;

/// Trace of a single configuration value.
pub use crate::trace::ValueTrace;

/// Builder for collecting trace data during config building.
pub use crate::trace::TraceBuilder;

// ============================================================================
// Pretty printing
// ============================================================================

/// Options for pretty printing errors.
pub use crate::pretty::PrettyPrintOptions;

/// Color output option.
pub use crate::pretty::ColorOption;

/// Trait extension for easy error handling with pretty printing.
///
/// Provides `unwrap_or_exit()` for CLI applications.
pub use crate::pretty::ValidationExt;

// ============================================================================
// Optional features
// ============================================================================

/// Hot-reloadable configuration (requires `watch` feature).
#[cfg(feature = "watch")]
pub use crate::watch::WatchedConfig;

/// File watcher for configuration changes (requires `watch` feature).
#[cfg(feature = "watch")]
pub use crate::watch::ConfigWatcher;

/// Events emitted during configuration watching (requires `watch` feature).
#[cfg(feature = "watch")]
pub use crate::watch::ConfigEvent;

// ============================================================================
// Derive macro
// ============================================================================

/// Derive macro for `Validate` trait (requires `derive` feature).
#[cfg(feature = "derive")]
pub use premortem_derive::Validate as DeriveValidate;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prelude_validation_types_available() {
        // Verify stillwater types are accessible
        let _: ConfigValidation<()> = Validation::Success(());
        let _: ConfigValidation<()> =
            Validation::Failure(ConfigErrors::single(ConfigError::NoSources));
    }

    #[test]
    fn test_prelude_error_types_available() {
        let err = ConfigError::NoSources;
        let errors = ConfigErrors::single(err);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_prelude_source_location_available() {
        let loc = SourceLocation::new("config.toml");
        assert_eq!(loc.source, "config.toml");
    }

    #[test]
    fn test_prelude_semigroup_combine() {
        let e1 = ConfigErrors::single(ConfigError::NoSources);
        let e2 = ConfigErrors::single(ConfigError::MissingField {
            path: "host".to_string(),
            source_location: None,
            searched_sources: vec![],
        });
        let combined = e1.combine(e2);
        assert_eq!(combined.len(), 2);
    }

    #[test]
    fn test_prelude_validation_all_vec() {
        let v1: ConfigValidation<i32> = Validation::Success(1);
        let v2: ConfigValidation<i32> = Validation::Success(2);

        let result = Validation::all_vec(vec![v1, v2]);
        assert!(result.is_success());
    }

    #[test]
    fn test_prelude_validation_all_vec_accumulates_errors() {
        let v1: ConfigValidation<i32> =
            Validation::Failure(ConfigErrors::single(ConfigError::NoSources));
        let v2: ConfigValidation<i32> =
            Validation::Failure(ConfigErrors::single(ConfigError::MissingField {
                path: "host".to_string(),
                source_location: None,
                searched_sources: vec![],
            }));

        let result = Validation::all_vec(vec![v1, v2]);
        assert!(result.is_failure());

        if let Validation::Failure(errors) = result {
            assert_eq!(errors.len(), 2);
        }
    }

    #[test]
    fn test_prelude_nonemptyvec_available() {
        let nev = NonEmptyVec::singleton(42);
        assert_eq!(*nev.head(), 42);
    }

    #[test]
    fn test_prelude_config_validation_ext() {
        let result: ConfigValidation<i32> = ConfigValidation::fail_with(ConfigError::NoSources);
        assert!(result.is_failure());
    }

    #[test]
    fn test_prelude_value_types_available() {
        let value = Value::String("test".to_string());
        assert_eq!(value.as_str(), Some("test"));
    }

    #[test]
    fn test_prelude_validators_module() {
        use validators::NonEmpty;

        let validator = NonEmpty;
        let result = validator.validate("hello", "field");
        assert!(result.is_success());
    }
}
