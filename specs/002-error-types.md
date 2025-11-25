---
number: 2
title: Error Types and Source Location
category: foundation
priority: critical
status: draft
dependencies: []
created: 2025-11-25
---

# Specification 002: Error Types and Source Location

**Category**: foundation
**Priority**: critical
**Status**: draft
**Dependencies**: None (can be implemented in parallel with 001)

## Context

A key differentiator of premortem is its rich error reporting. Every error must carry enough context to pinpoint exactly where a problem occurred - which file, which line, which environment variable. This enables the "all errors at once" experience with clear attribution.

The error types must support:
- Source loading failures (file not found, parse errors)
- Type conversion failures (expected integer, got string)
- Missing required fields
- Validation failures (range, format, custom)
- Cross-field validation failures
- Unknown field warnings/errors

## Objective

Implement comprehensive error types with source location tracking that enable clear, actionable error messages showing exactly where each problem originated.

## Requirements

### Functional Requirements

1. **ConfigError Enum**: Comprehensive error type covering all failure modes
2. **SourceLocation**: Tracks source name, line, and column
3. **Display Formatting**: Human-readable error messages
4. **Error Grouping**: Support for grouping errors by source
5. **Suggestions**: "Did you mean?" suggestions for unknown fields
6. **Sensitive Redaction**: Ability to redact values marked as sensitive

### Non-Functional Requirements

- Errors must be `Clone` for accumulation in `Validation`
- Errors must implement `std::error::Error`
- Error messages must be actionable and specific
- Source location should be optional (not all sources have line numbers)

## Acceptance Criteria

- [ ] `ConfigError` enum with variants for all error types
- [ ] `SourceLocation` struct with source name, optional line/column
- [ ] `Display` implementation produces clear, formatted messages
- [ ] `Error` trait implementation with proper source chaining
- [ ] Helper constructors for common error cases
- [ ] `path()` method returns the config path that failed
- [ ] Errors can be grouped by source for reporting
- [ ] Unit tests for all error variants and formatting

## Technical Details

### Error Type Definition

```rust
/// Errors that can occur during configuration loading and validation
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// A configuration source failed to load
    SourceError {
        source_name: String,
        kind: SourceErrorKind,
    },

    /// A field failed to parse to the expected type
    ParseError {
        path: String,
        source_location: SourceLocation,
        expected_type: String,
        actual_value: String,
        message: String,
    },

    /// A required field is missing
    MissingField {
        path: String,
        searched_sources: Vec<String>,
    },

    /// A validation rule failed
    ValidationError {
        path: String,
        source_location: Option<SourceLocation>,
        value: Option<String>,  // None if sensitive
        message: String,
    },

    /// Cross-field validation failed
    CrossFieldError {
        paths: Vec<String>,
        message: String,
    },

    /// An unknown field was found (when configured to error)
    UnknownField {
        path: String,
        source_location: SourceLocation,
        did_you_mean: Option<String>,
    },
}

/// Kinds of source loading errors
#[derive(Debug, Clone)]
pub enum SourceErrorKind {
    /// Source file was not found
    NotFound { path: String },
    /// Source file could not be read
    IoError { message: String },
    /// Source content could not be parsed
    ParseError { message: String, line: Option<u32>, column: Option<u32> },
    /// Remote source failed to connect
    ConnectionError { message: String },
    /// Other source-specific error
    Other { message: String },
}

/// Location where a configuration value originated
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// Name of the source (e.g., "config.toml", "env:APP_HOST")
    pub source: String,
    /// Line number in the source (1-indexed), if applicable
    pub line: Option<u32>,
    /// Column number in the source (1-indexed), if applicable
    pub column: Option<u32>,
}
```

### Display Implementation

```rust
impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingField { path, searched_sources } => {
                write!(f, "missing required field '{}' (searched: {})",
                    path,
                    searched_sources.join(", "))
            }
            ConfigError::ParseError { path, source_location, expected_type, actual_value, message } => {
                write!(f, "[{}] '{}': expected {}, got \"{}\": {}",
                    source_location, path, expected_type, actual_value, message)
            }
            ConfigError::ValidationError { path, source_location, message, .. } => {
                match source_location {
                    Some(loc) => write!(f, "[{}] '{}': {}", loc, path, message),
                    None => write!(f, "'{}': {}", path, message),
                }
            }
            ConfigError::CrossFieldError { paths, message } => {
                write!(f, "[{}]: {}", paths.join(", "), message)
            }
            ConfigError::UnknownField { path, source_location, did_you_mean } => {
                let mut msg = format!("[{}] unknown field '{}'", source_location, path);
                if let Some(suggestion) = did_you_mean {
                    msg.push_str(&format!("; did you mean '{}'?", suggestion));
                }
                write!(f, "{}", msg)
            }
            ConfigError::SourceError { source_name, kind } => {
                write!(f, "{}: {}", source_name, kind)
            }
        }
    }
}

impl std::fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(col)) => write!(f, "{}:{}:{}", self.source, line, col),
            (Some(line), None) => write!(f, "{}:{}", self.source, line),
            _ => write!(f, "{}", self.source),
        }
    }
}
```

### Helper Methods

```rust
impl ConfigError {
    /// Get the configuration path that this error relates to, if any
    pub fn path(&self) -> Option<&str> {
        match self {
            ConfigError::ParseError { path, .. } => Some(path),
            ConfigError::MissingField { path, .. } => Some(path),
            ConfigError::ValidationError { path, .. } => Some(path),
            ConfigError::UnknownField { path, .. } => Some(path),
            ConfigError::CrossFieldError { .. } => None,
            ConfigError::SourceError { .. } => None,
        }
    }

    /// Get the source location of this error, if any
    pub fn source_location(&self) -> Option<&SourceLocation> {
        match self {
            ConfigError::ParseError { source_location, .. } => Some(source_location),
            ConfigError::ValidationError { source_location, .. } => source_location.as_ref(),
            ConfigError::UnknownField { source_location, .. } => Some(source_location),
            _ => None,
        }
    }

    /// Check if this is a validation error
    pub fn is_validation_error(&self) -> bool {
        matches!(self, ConfigError::ValidationError { .. } | ConfigError::CrossFieldError { .. })
    }

    /// Get a suggestion for fixing this error, if available
    pub fn suggestion(&self) -> Option<String> {
        match self {
            ConfigError::UnknownField { did_you_mean: Some(s), path, .. } => {
                Some(format!("Change '{}' to '{}'", path, s))
            }
            ConfigError::MissingField { path, .. } => {
                Some(format!("Add '{}' to your configuration", path))
            }
            _ => None,
        }
    }
}

impl SourceLocation {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            line: None,
            column: None,
        }
    }

    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    pub fn with_column(mut self, column: u32) -> Self {
        self.column = Some(column);
        self
    }

    /// Create a location for an environment variable
    pub fn env(var_name: &str) -> Self {
        Self::new(format!("env:{}", var_name))
    }

    /// Create a location for a file with optional position
    pub fn file(path: &str, line: Option<u32>, column: Option<u32>) -> Self {
        Self {
            source: path.to_string(),
            line,
            column,
        }
    }
}
```

### Error Grouping Utilities

```rust
/// Group errors by their source for organized reporting
pub fn group_by_source(errors: &[ConfigError]) -> BTreeMap<String, Vec<&ConfigError>> {
    let mut groups: BTreeMap<String, Vec<&ConfigError>> = BTreeMap::new();

    for error in errors {
        let source = error.source_location()
            .map(|loc| loc.source.clone())
            .unwrap_or_else(|| "(general)".to_string());

        groups.entry(source).or_default().push(error);
    }

    groups
}
```

## Dependencies

- **Prerequisites**: None
- **Affected Components**: Used by all other specs
- **External Dependencies**:
  - `std::error::Error` trait
  - `strsim` crate for "did you mean" suggestions (optional)

## Testing Strategy

- **Unit Tests**:
  - All error variant construction
  - Display formatting for each variant
  - Source location formatting
  - Path extraction
  - Error grouping
- **Integration Tests**: Used implicitly by other spec tests

## Documentation Requirements

- **Code Documentation**: Doc comments with examples for each error type
- **User Documentation**: Guide on interpreting error messages

## Implementation Notes

- Consider using `thiserror` for `Error` trait derivation
- `strsim` crate provides Levenshtein distance for "did you mean"
- Keep `Clone` cheap - avoid storing large data in errors
- Consider `Arc<str>` instead of `String` for frequently cloned strings

## Migration and Compatibility

Not applicable - new project.
