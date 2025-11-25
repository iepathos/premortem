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
//! - **Source Layering**: Merge config from files, environment, and remote sources
//! - **Testable I/O**: Dependency injection via `ConfigEnv` trait
//! - **Type Safety**: Deserialize into your types with full validation
//!
//! # Quick Start
//!
//! ```ignore
//! use premortem::{Config, Validate, ConfigValidation};
//! use serde::Deserialize;
//! use stillwater::Validation;
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
//!             Validation::Failure(ConfigErrors::single(ConfigError::ValidationError {
//!                 path: "port".to_string(),
//!                 source_location: None,
//!                 value: Some(self.port.to_string()),
//!                 message: "port must be positive".to_string(),
//!             }))
//!         }
//!     }
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
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
//! # Architecture
//!
//! Premortem follows the "pure core, imperative shell" pattern:
//!
//! - **Pure Core**: Value merging, deserialization, and validation are pure functions
//! - **Imperative Shell**: I/O operations use stillwater's `Effect` type with `ConfigEnv`
//!
//! This architecture enables:
//! - Easy unit testing with `MockEnv`
//! - Composable validation with error accumulation
//! - Clear separation of concerns
//!
//! # Module Structure
//!
//! - [`config`]: `Config` and `ConfigBuilder` for loading configuration
//! - [`error`]: Error types (`ConfigError`, `ConfigErrors`, `ConfigValidation`)
//! - [`value`]: `Value` enum for intermediate representation
//! - [`source`]: `Source` trait and `ConfigValues` container
//! - [`mod@env`]: `ConfigEnv` trait and `MockEnv` for testing
//! - [`validate`]: `Validate` trait for custom validation

pub mod config;
pub mod env;
pub mod error;
pub mod source;
pub mod sources;
pub mod validate;
pub mod value;

// Re-exports for convenience
pub use config::{Config, ConfigBuilder};
pub use env::{ConfigEnv, MockEnv, RealEnv};
pub use error::{
    group_by_source, ConfigError, ConfigErrors, ConfigValidation, ConfigValidationExt,
    SourceErrorKind, SourceLocation,
};
pub use source::{merge_config_values, ConfigValues, Source};
pub use validate::validators;
pub use validate::{
    custom, validate_field, validate_nested, validate_optional_nested, Validate, Validator, When,
};
pub use value::{ConfigValue, Value};

// Re-export sources
pub use sources::Env;
#[cfg(feature = "toml")]
pub use sources::Toml;
pub use sources::{Defaults, PartialDefaults};

// Re-export stillwater types that are commonly used
pub use stillwater::{NonEmptyVec, Semigroup, Validation};

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
