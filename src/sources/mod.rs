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

/// Calculate line number from byte offset (1-indexed).
/// Pure function used by file-based sources for line tracking.
pub fn line_from_offset(content: &str, offset: usize) -> u32 {
    content[..offset.min(content.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count() as u32
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_from_offset_first_line() {
        let content = "first line\nsecond line\nthird line";
        // First character is on line 1
        assert_eq!(line_from_offset(content, 0), 1);
        // Still on line 1
        assert_eq!(line_from_offset(content, 5), 1);
    }

    #[test]
    fn test_line_from_offset_second_line() {
        let content = "first line\nsecond line\nthird line";
        // After first newline, on line 2
        assert_eq!(line_from_offset(content, 11), 2);
        assert_eq!(line_from_offset(content, 15), 2);
    }

    #[test]
    fn test_line_from_offset_third_line() {
        let content = "first line\nsecond line\nthird line";
        // After second newline, on line 3
        assert_eq!(line_from_offset(content, 23), 3);
    }

    #[test]
    fn test_line_from_offset_empty_content() {
        let content = "";
        assert_eq!(line_from_offset(content, 0), 1);
    }

    #[test]
    fn test_line_from_offset_beyond_content() {
        let content = "short";
        // Should clamp to content length
        assert_eq!(line_from_offset(content, 100), 1);
    }
}
