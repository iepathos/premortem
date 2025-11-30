// Allow large error types - detailed config errors are expected
#![allow(clippy::result_large_err)]

//! Premortem: A configuration library that performs a premortem on your app's config.
//!
//! Premortem validates your configuration before your application runs, finding all
//! the ways it could die from bad config upfront. It uses stillwater's functional
//! patterns for error accumulation and composable validation.
//!
//! # Core Concepts
//!
//! - **Error Accumulation**: Find ALL configuration errors, not just the first one
//! - **Source Layering**: Merge config from files and environment variables
//! - **Required Variables**: Declarative validation of required environment variables
//! - **Testable I/O**: Dependency injection via `ConfigEnv` trait
//! - **Type Safety**: Deserialize into your types with full validation
//!
//! # Quick Start
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
//! For most users, import the prelude:
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
//! use stillwater::Effect;  // Direct stillwater access
//! ```
//!
//! # Required Environment Variables
//!
//! Mark environment variables as required at the source level with error accumulation:
//!
//! ```ignore
//! use premortem::prelude::*;
//!
//! let config = Config::<AppConfig>::builder()
//!     .source(
//!         Env::prefix("APP_")
//!             .require_all(&["JWT_SECRET", "DATABASE_URL", "API_KEY"])
//!     )
//!     .build()?;
//! ```
//!
//! All missing required variables are reported together:
//!
//! ```text
//! Configuration errors (3):
//!   [env:APP_JWT_SECRET] Missing required field: jwt.secret
//!   [env:APP_DATABASE_URL] Missing required field: database.url
//!   [env:APP_API_KEY] Missing required field: api.key
//! ```
//!
//! This separates **presence validation** (does the variable exist?) from
//! **value validation** (does it meet constraints?).
//!
//! # Architecture
//!
//! Premortem follows the "pure core, imperative shell" pattern:
//!
//! - **Pure Core**: Value merging, deserialization, and validation are pure functions
//! - **Imperative Shell**: I/O operations use the `ConfigEnv` trait for dependency injection
//!
//! This architecture enables:
//! - Easy unit testing with `MockEnv`
//! - Composable validation with error accumulation
//! - Clear separation of concerns
//!
//! # Module Structure
//!
//! - [`prelude`]: Convenient re-exports for common usage
//! - [`config`]: `Config` and `ConfigBuilder` for loading configuration
//! - [`error`]: Error types (`ConfigError`, `ConfigErrors`, `ConfigValidation`)
//! - [`value`]: `Value` enum for intermediate representation
//! - [`source`]: `Source` trait and `ConfigValues` container
//! - [`mod@env`]: `ConfigEnv` trait and `MockEnv` for testing
//! - [`validate`]: `Validate` trait for custom validation
//!
//! # Stillwater Integration
//!
//! Premortem uses these stillwater types:
//!
//! | Type | Usage |
//! |------|-------|
//! | `Validation<T, E>` | Error accumulation for config errors |
//! | `NonEmptyVec<T>` | Guaranteed non-empty error lists |
//! | `Semigroup` | Combining errors from multiple sources |
//!
//! These are re-exported from the prelude for convenience.

pub mod config;
pub mod env;
pub mod error;
pub mod prelude;
pub mod pretty;
pub mod source;
pub mod sources;
pub mod trace;
pub mod validate;
pub mod value;
#[cfg(feature = "watch")]
pub mod watch;

// Re-exports for convenience
pub use config::{Config, ConfigBuilder};
pub use env::{ConfigEnv, MockEnv, RealEnv};
pub use error::{
    group_by_source, ConfigError, ConfigErrors, ConfigValidation, ConfigValidationExt,
    SourceErrorKind, SourceLocation,
};
pub use pretty::{ColorOption, PrettyPrintOptions, ValidationExt};
pub use source::{merge_config_values, ConfigValues, Source};
pub use trace::{TraceBuilder, TracedConfig, TracedValue, ValueTrace};
pub use validate::validators;
pub use validate::{
    current_source_location, custom, from_predicate, validate_field, validate_nested,
    validate_optional_nested, validate_with_predicate, with_validation_context, SourceLocationMap,
    Validate, ValidationContext, Validator, When,
};
pub use value::{ConfigValue, Value};

// Re-export sources
pub use sources::Env;
#[cfg(feature = "json")]
pub use sources::Json;
#[cfg(feature = "toml")]
pub use sources::Toml;
#[cfg(feature = "yaml")]
pub use sources::Yaml;
pub use sources::{Defaults, PartialDefaults};

// Re-export watch types
#[cfg(feature = "watch")]
pub use watch::{ConfigEvent, ConfigWatcher, WatchedConfig};

// Re-export stillwater types that are commonly used
pub use stillwater::{NonEmptyVec, Semigroup, Validation};

// Re-export stillwater predicate module and types (0.13.0+)
pub use stillwater::predicate::{self, Predicate, PredicateExt};

// Re-export derive macro when the feature is enabled
#[cfg(feature = "derive")]
pub use premortem_derive::Validate as DeriveValidate;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reexports() {
        // Ensure all re-exports are accessible
        let _: ConfigValidation<()> = Validation::Success(());
    }
}
