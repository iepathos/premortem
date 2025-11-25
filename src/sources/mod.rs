//! Configuration source implementations.
//!
//! This module contains implementations of the `Source` trait for various
//! configuration formats and locations.

mod defaults;
mod env_source;
#[cfg(feature = "json")]
mod json_source;
#[cfg(feature = "toml")]
mod toml_source;

pub use defaults::{Defaults, PartialDefaults};
pub use env_source::Env;
#[cfg(feature = "json")]
pub use json_source::Json;
#[cfg(feature = "toml")]
pub use toml_source::Toml;
