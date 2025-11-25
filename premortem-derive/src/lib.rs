//! Derive macros for the premortem configuration validation library.
//!
//! This crate provides the `#[derive(Validate)]` macro that generates
//! `Validate` trait implementations using stillwater's `Validation::all()`
//! pattern for error accumulation.
//!
//! # Basic Usage
//!
//! ```ignore
//! use premortem::Validate;
//!
//! #[derive(Validate)]
//! struct ServerConfig {
//!     #[validate(non_empty)]
//!     host: String,
//!
//!     #[validate(range(1..=65535))]
//!     port: u16,
//! }
//! ```
//!
//! # Available Validators
//!
//! ## String Validators
//! - `non_empty` - Value cannot be empty
//! - `min_length(n)` - Minimum string length
//! - `max_length(n)` - Maximum string length
//! - `length(n..=m)` - String length in range
//! - `pattern("regex")` - Match regex pattern
//! - `email` - Valid email format
//! - `url` - Valid URL format
//! - `ip` - Valid IP address
//! - `uuid` - Valid UUID format
//!
//! ## Numeric Validators
//! - `range(n..=m)` - Value in range
//! - `positive` - Value > 0
//! - `negative` - Value < 0
//! - `non_zero` - Value != 0
//!
//! ## Path Validators
//! - `file_exists` - File exists at path
//! - `dir_exists` - Directory exists at path
//! - `parent_exists` - Parent directory exists
//! - `extension("ext")` - File has extension
//!
//! ## Collection Validators
//! - `each(validator)` - Apply validator to each element
//!
//! ## Structural Validators
//! - `nested` - Validate nested struct
//! - `skip` - Skip validation for this field
//! - `custom = "fn_name"` - Call custom validation function
//!
//! ## Control Flow
//! - `when = "condition"` - Conditional validation
//! - `message = "custom message"` - Override error message
//!
//! # Sensitive Fields
//!
//! Use `#[sensitive]` to redact field values in error messages:
//!
//! ```ignore
//! #[derive(Validate)]
//! struct Credentials {
//!     #[sensitive]
//!     #[validate(min_length(8))]
//!     password: String,
//! }
//! ```
//!
//! # Struct-Level Validation
//!
//! Use `#[validate(custom = "fn_name")]` on the struct for cross-field validation:
//!
//! ```ignore
//! #[derive(Validate)]
//! #[validate(custom = "validate_config")]
//! struct Config {
//!     start_port: u16,
//!     end_port: u16,
//! }
//!
//! fn validate_config(cfg: &Config) -> ConfigValidation<()> {
//!     if cfg.start_port < cfg.end_port {
//!         Validation::Success(())
//!     } else {
//!         Validation::fail_with(ConfigError::CrossFieldError {
//!             paths: vec!["start_port".into(), "end_port".into()],
//!             message: "start_port must be less than end_port".into(),
//!         })
//!     }
//! }
//! ```

extern crate proc_macro;

mod codegen;
mod parse;
mod validate;
mod validators;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

/// Derive the `Validate` trait for a struct.
///
/// This macro generates an implementation of `premortem::Validate` that
/// validates all fields according to their `#[validate(...)]` attributes.
///
/// The generated code uses stillwater's `Validation::all()` to run all
/// validations in parallel and accumulate ALL errors, not just the first one.
///
/// # Example
///
/// ```ignore
/// use premortem::Validate;
///
/// #[derive(Validate)]
/// struct DatabaseConfig {
///     #[validate(non_empty, message = "Host is required")]
///     host: String,
///
///     #[validate(range(1..=65535))]
///     port: u16,
///
///     #[validate(positive)]
///     pool_size: u32,
///
///     #[validate(nested)]
///     tls: Option<TlsConfig>,
/// }
/// ```
#[proc_macro_derive(Validate, attributes(validate, sensitive))]
pub fn derive_validate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match validate::derive_validate(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
