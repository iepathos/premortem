//! Configuration source implementations.
//!
//! This module contains implementations of the `Source` trait for various
//! configuration formats and locations.

#[cfg(feature = "toml")]
mod toml_source;

#[cfg(feature = "toml")]
pub use toml_source::Toml;
